//! The native half of on-device dictation.
//!
//! Ported from `apps/desktop/src-tauri/src/speech.rs` with the cloud engine left
//! out (it reads the Codex sign-in, which this slice is not allowed to touch).
//! What is left is what the React app calls "on-device": one worker thread that
//! owns the recognizer, the microphone and the `DictationLifecycle` from
//! `retinue-speech`, so the two windows can never drive the same session.
//!
//! TODO(owner): cloud dictation is out of scope for slice 5. It depends on the
//! owner-approved exception that lets the installed app read the Codex sign-in,
//! and it needs the owner's go-ahead for the native app before any of that code
//! is ported. Nothing here reads or copies a credential file, and this module
//! never sends audio anywhere: the only engine is the local one.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use retinue_speech::capture::{start_capture, AudioFrame};
use retinue_speech::{
    catalog, downmix_to_mono, get_catalog_model, model_manager, resample_to_rate, rms_level,
    DictationLifecycle, DictationOwner, EngineEvent, SpeechEngine, SpeechModelManifest,
};

use crate::paths;
use crate::trace;

pub use retinue_speech::DictationOwner as Owner;

/// How long a warm worker waits for the next session before releasing the model.
const IDLE_WORKER_TEARDOWN: Duration = Duration::from_secs(600);
/// Throttle for microphone level updates (~16 Hz).
const LEVEL_INTERVAL: Duration = Duration::from_millis(60);

/// What the engine tells the window. The `text` of a `Partial` or a `Final` is
/// the transcript, and it is only ever handed to the window.
#[derive(Clone, Debug, PartialEq)]
pub enum DictationEvent {
    /// The microphone is open; words may follow.
    Ready,
    Loading,
    Loaded,
    Partial(String),
    Final(String),
    Processing,
    Stopped,
    Error(String),
    /// The microphone's loudness, 0 to 1, for the island's bars.
    Level(f32),
}

/// One catalog model, and whether this machine already has it.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelStatus {
    pub model_id: String,
    pub label: String,
    pub ready: bool,
    pub size_bytes: Option<u64>,
    /// Where the files would live. `None` when the id is not in the catalog.
    pub directory: Option<PathBuf>,
}

/// The models the catalogue offers, for slice 6's Settings page.
pub fn catalog_models() -> Vec<ModelStatus> {
    catalog()
        .iter()
        .filter(|manifest| manifest.is_local())
        .map(status_of)
        .collect()
}

/// One model's status, or `None` when the id is not in the pinned catalog.
pub fn model_status(model_id: &str) -> Option<ModelStatus> {
    get_catalog_model(model_id).map(status_of)
}

fn status_of(manifest: &SpeechModelManifest) -> ModelStatus {
    let root = model_root(manifest);
    let directory = model_manager::model_directory(&root, &manifest.id);
    ModelStatus {
        model_id: manifest.id.clone(),
        label: manifest.label.clone(),
        ready: model_manager::model_is_ready(&directory, manifest),
        size_bytes: Some(manifest.size_bytes),
        directory: Some(directory),
    }
}

/// Where the speech models live.
///
/// The installed app's folder comes first, so a model it already downloaded is
/// reused rather than fetched again (`resolve_cache_root` in `speech.rs`); this
/// app's own folder is the fallback. sherpa-onnx cannot load a model from a
/// non-ASCII path, so a path that has one falls back to a shared ASCII folder
/// under `ProgramData`, exactly as the installed app does.
pub fn model_root_for(base: PathBuf) -> PathBuf {
    if !has_non_ascii(&base.to_string_lossy()) {
        return base;
    }
    let digest = model_manager::sha256_bytes(base.to_string_lossy().as_bytes());
    let short = &digest[..16];
    std::env::var("ProgramData")
        .or_else(|_| std::env::var("PROGRAMDATA"))
        .ok()
        .map(|root| PathBuf::from(root).join("Retinue").join("speech-models").join(short))
        .filter(|candidate| !has_non_ascii(&candidate.to_string_lossy()))
        .unwrap_or(base)
}

fn model_root(manifest: &SpeechModelManifest) -> PathBuf {
    let installed = paths::local_app_data().join(paths::INSTALLED_DIR_NAME).join("speech-models");
    let directory = model_manager::model_directory(&installed, &manifest.id);
    if model_manager::model_is_ready(&directory, manifest) {
        return model_root_for(installed);
    }
    model_root_for(paths::native_data_dir().join("speech-models"))
}

fn has_non_ascii(value: &str) -> bool {
    !value.is_ascii()
}

enum WorkerCommand {
    Start { owner: DictationOwner },
    Stop,
    Teardown,
    Frame(AudioFrame),
}

struct WorkerHandle {
    commands: Sender<WorkerCommand>,
    join: Option<JoinHandle<()>>,
    model_id: String,
}

/// The engine the window holds: a lifecycle, one worker and an event stream.
pub struct DictationEngine {
    lifecycle: Arc<Mutex<DictationLifecycle>>,
    worker: Mutex<Option<WorkerHandle>>,
    events: async_channel::Sender<DictationEvent>,
}

impl DictationEngine {
    pub fn new(events: async_channel::Sender<DictationEvent>) -> Self {
        Self { lifecycle: Arc::new(Mutex::new(DictationLifecycle::new())), worker: Mutex::new(None), events }
    }

    /// Start one session for `owner`, opening the microphone at once. The model
    /// loads behind it, so the first words are queued, not lost.
    pub fn start(&self, model_id: &str, owner: DictationOwner) -> Result<(), String> {
        let manifest = get_catalog_model(model_id).cloned().ok_or_else(|| "unknown speech model".to_owned())?;
        let root = model_root(&manifest);
        let directory = model_manager::model_directory(&root, model_id);
        if !model_manager::model_is_ready(&directory, &manifest) {
            return Err("speech_model_not_ready".to_owned());
        }
        self.lifecycle
            .lock()
            .map_err(|_| "speech state is unavailable".to_owned())?
            .begin_start(owner, model_id)
            .map_err(|error| error.to_string())?;

        self.reap_finished();
        let reusable = self
            .worker
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().and_then(|handle| (handle.model_id == model_id && worker_alive(handle)).then(|| handle.commands.clone())));
        if let Some(commands) = reusable {
            if commands.send(WorkerCommand::Start { owner }).is_ok() {
                if let Ok(mut lifecycle) = self.lifecycle.lock() {
                    let _ = lifecycle.mark_active(owner);
                }
                return Ok(());
            }
        }
        self.teardown_worker();
        self.spawn_worker(Some(manifest), Some(directory), model_id.to_owned(), Some(owner))?;
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            let _ = lifecycle.mark_active(owner);
        }
        Ok(())
    }

    /// Load the model in the background, so the first dictation starts at once.
    pub fn warm(&self, model_id: &str) -> bool {
        let Some(manifest) = get_catalog_model(model_id).cloned() else { return false };
        let root = model_root(&manifest);
        let directory = model_manager::model_directory(&root, model_id);
        if !model_manager::model_is_ready(&directory, &manifest) {
            return false;
        }
        if self.lifecycle.lock().map(|lifecycle| lifecycle.phase_name() != "idle").unwrap_or(true) {
            return false;
        }
        self.reap_finished();
        let warm = self
            .worker
            .lock()
            .ok()
            .map(|guard| guard.as_ref().is_some_and(|handle| handle.model_id == model_id && worker_alive(handle)))
            .unwrap_or(false);
        if warm {
            return true;
        }
        self.teardown_worker();
        self.spawn_worker(Some(manifest), Some(directory), model_id.to_owned(), None).is_ok()
    }

    pub fn stop(&self, owner: DictationOwner) -> Result<(), String> {
        let was_active = self
            .lifecycle
            .lock()
            .map(|lifecycle| lifecycle.active_owner() == Some(owner))
            .unwrap_or(false);
        self.lifecycle
            .lock()
            .map_err(|_| "speech state is unavailable".to_owned())?
            .begin_stop(owner)
            .map_err(|error| error.to_string())?;
        if was_active {
            self.notify(DictationEvent::Processing);
            if let Ok(guard) = self.worker.lock() {
                if let Some(handle) = guard.as_ref() {
                    let _ = handle.commands.send(WorkerCommand::Stop);
                }
            }
        }
        Ok(())
    }

    /// Drop a session without keeping the words (Escape).
    pub fn cancel(&self, owner: DictationOwner) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            let _ = lifecycle.begin_stop(owner);
        }
        if let Ok(guard) = self.worker.lock() {
            if let Some(handle) = guard.as_ref() {
                let _ = handle.commands.send(WorkerCommand::Stop);
            }
        }
    }

    /// Free the model, for example when dictation is switched off.
    pub fn release(&self) {
        if self.lifecycle.lock().map(|lifecycle| lifecycle.phase_name() != "idle").unwrap_or(false) {
            return;
        }
        self.teardown_worker();
    }

    pub fn phase(&self) -> &'static str {
        self.lifecycle.lock().map(|lifecycle| lifecycle.phase_name()).unwrap_or("idle")
    }

    pub fn active_owner(&self) -> Option<DictationOwner> {
        self.lifecycle.lock().ok().and_then(|lifecycle| lifecycle.active_owner())
    }

    fn notify(&self, event: DictationEvent) {
        let _ = self.events.try_send(event);
    }

    fn reap_finished(&self) {
        let mut guard = match self.worker.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let finished = guard.as_ref().map(|handle| !worker_alive(handle)).unwrap_or(false);
        if finished {
            if let Some(mut handle) = guard.take() {
                if let Some(join) = handle.join.take() {
                    let _ = join.join();
                }
            }
        }
    }

    fn teardown_worker(&self) {
        let handle = self.worker.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut worker) = handle {
            let _ = worker.commands.send(WorkerCommand::Teardown);
            if let Some(join) = worker.join.take() {
                let _ = join.join();
            }
        }
    }

    fn spawn_worker(
        &self,
        manifest: Option<SpeechModelManifest>,
        directory: Option<PathBuf>,
        model_id: String,
        start: Option<DictationOwner>,
    ) -> Result<(), String> {
        let (commands, receiver) = mpsc::channel::<WorkerCommand>();
        let capture_commands = commands.clone();
        let events = self.events.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        let join = thread::Builder::new()
            .name("retinue-dictation".to_owned())
            .spawn(move || run_worker(events, manifest, directory, lifecycle, receiver, capture_commands, start))
            .map_err(|error| error.to_string())?;
        *self.worker.lock().map_err(|_| "speech state is unavailable".to_owned())? =
            Some(WorkerHandle { commands, join: Some(join), model_id });
        Ok(())
    }
}

fn worker_alive(handle: &WorkerHandle) -> bool {
    handle.join.as_ref().map(|join| !join.is_finished()).unwrap_or(false)
}

fn end_session(lifecycle: &Arc<Mutex<DictationLifecycle>>, owner: DictationOwner) {
    if let Ok(mut lifecycle) = lifecycle.lock() {
        let _ = lifecycle.begin_stop(owner);
    }
}

fn load_engine(manifest: &Option<SpeechModelManifest>, directory: &Option<PathBuf>) -> Result<SpeechEngine, String> {
    match (manifest.as_ref(), directory.as_ref()) {
        (Some(manifest), Some(directory)) => SpeechEngine::load(manifest, directory).map_err(|error| error.to_string()),
        _ => Err("speech_model_not_ready".to_owned()),
    }
}

fn emit(events: &async_channel::Sender<DictationEvent>, event: DictationEvent) {
    let _ = events.try_send(event);
}

/// The worker: one session at a time, the microphone open before the model is
/// loaded, and the model kept warm between sessions.
fn run_worker(
    events: async_channel::Sender<DictationEvent>,
    manifest: Option<SpeechModelManifest>,
    directory: Option<PathBuf>,
    lifecycle: Arc<Mutex<DictationLifecycle>>,
    receiver: Receiver<WorkerCommand>,
    capture_commands: Sender<WorkerCommand>,
    start: Option<DictationOwner>,
) {
    let mut engine: Option<SpeechEngine> = None;
    if start.is_none() {
        // Warm-up: load now. A failure is reported by the next real start.
        engine = load_engine(&manifest, &directory).ok();
    }
    let mut pending = start;
    let mut last_level = Instant::now();
    loop {
        let owner = match pending.take() {
            Some(owner) => owner,
            None => match receiver.recv_timeout(IDLE_WORKER_TEARDOWN) {
                Ok(WorkerCommand::Start { owner }) => owner,
                Ok(WorkerCommand::Teardown) | Err(_) => break,
                Ok(_) => continue,
            },
        };

        // The microphone opens before the model is ready: audio queues in the
        // channel while it loads, so the first words are never lost.
        let commands = capture_commands.clone();
        let capture = match start_capture(move |frame| {
            let _ = commands.send(WorkerCommand::Frame(frame));
        }) {
            Ok(capture) => capture,
            Err(error) => {
                emit(&events, DictationEvent::Error(error.to_string()));
                end_session(&lifecycle, owner);
                continue;
            }
        };
        trace::log(format!("dictation: listening ({})", owner.as_str()));
        emit(&events, DictationEvent::Ready);

        if engine.is_none() {
            emit(&events, DictationEvent::Loading);
            match load_engine(&manifest, &directory) {
                Ok(loaded) => {
                    engine = Some(loaded);
                    emit(&events, DictationEvent::Loaded);
                    trace::log("dictation: model loaded");
                }
                Err(error) => {
                    capture.stop();
                    emit(&events, DictationEvent::Error(error));
                    end_session(&lifecycle, owner);
                    continue;
                }
            }
        }

        let Some(local) = engine.as_mut() else { continue };
        loop {
            match receiver.recv() {
                Ok(WorkerCommand::Frame(frame)) => {
                    let mono = downmix_to_mono(&frame.samples, frame.channels as usize);
                    let resampled = resample_to_rate(&mono, frame.sample_rate, local.sample_rate());
                    if last_level.elapsed() >= LEVEL_INTERVAL {
                        last_level = Instant::now();
                        emit(&events, DictationEvent::Level(rms_level(&resampled)));
                    }
                    for event in local.accept(&resampled) {
                        match event {
                            EngineEvent::Partial(text) => emit(&events, DictationEvent::Partial(text)),
                            EngineEvent::Final(text) => emit(&events, DictationEvent::Final(text)),
                        }
                    }
                }
                Ok(WorkerCommand::Stop) | Err(_) => {
                    capture.stop();
                    for event in local.finish() {
                        match event {
                            EngineEvent::Partial(text) => emit(&events, DictationEvent::Partial(text)),
                            EngineEvent::Final(text) => emit(&events, DictationEvent::Final(text)),
                        }
                    }
                    emit(&events, DictationEvent::Stopped);
                    trace::log("dictation: stopped");
                    local.reset();
                    break;
                }
                Ok(WorkerCommand::Teardown) => {
                    capture.stop();
                    let _ = local.finish();
                    emit(&events, DictationEvent::Stopped);
                    return;
                }
                Ok(WorkerCommand::Start { .. }) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_model_is_refused_and_the_catalog_answers_for_a_known_one() {
        assert!(model_status("no-such-model").is_none());
        let status = model_status(crate::dictation::settings::DEFAULT_SPEECH_MODEL_ID).expect("the default model is pinned");
        assert_eq!(status.label, "Parakeet TDT v3");
        // The size is the catalog's own total, so the bubble can name it.
        assert_eq!(status.size_bytes, Some(652_184_281 + 11_845_275 + 6_355_277 + 93_939));
    }

    #[test]
    fn only_local_models_are_offered() {
        let models = catalog_models();
        assert!(models.iter().any(|model| model.model_id == crate::dictation::settings::DEFAULT_SPEECH_MODEL_ID));
        assert!(models.iter().all(|model| model.size_bytes.is_some()));
    }

    #[test]
    fn an_ascii_model_folder_is_used_as_it_is_and_a_non_ascii_one_moves() {
        let plain = model_root_for(PathBuf::from(r"C:\Users\me\AppData\Local\Retinue\speech-models"));
        assert_eq!(plain, PathBuf::from(r"C:\Users\me\AppData\Local\Retinue\speech-models"));
        // An apostrophe is ASCII, and sherpa-onnx has no trouble with it; this
        // repository's own checkout path is full of them.
        let apostrophe = model_root_for(PathBuf::from(r"D:\Basem's\Coding\speech-models"));
        assert_eq!(apostrophe, PathBuf::from(r"D:\Basem's\Coding\speech-models"));
        // A non-ASCII folder moves to the shared one under ProgramData, or is
        // left alone when there is no ProgramData to move to.
        let moved = model_root_for(PathBuf::from("D:\\Básem\\speech-models"));
        let moved_text = moved.to_string_lossy().to_string();
        match std::env::var("ProgramData") {
            Ok(root) if root.is_ascii() => {
                assert!(moved_text.starts_with(&root), "{moved_text} should sit under ProgramData");
            }
            _ => assert_eq!(moved_text, "D:\\Básem\\speech-models"),
        }
    }

    #[test]
    fn starting_without_a_model_is_refused_with_the_react_apps_error_code() {
        let (events, _receiver) = async_channel::unbounded();
        let engine = DictationEngine::new(events);
        // A model id that exists but whose files are not here.
        let error = engine.start("parakeet-tdt-0.6b-v2-int8", DictationOwner::Companion).unwrap_err();
        assert!(error == "speech_model_not_ready" || error == "unknown speech model", "{error}");
        assert_eq!(engine.phase(), "idle", "a refused start leaves no session behind");
    }
}
