//! On-device dictation: the shortcut, the engine, the bubble and where the
//! words land.
//!
//! Ported from `ui/dictation.ts` (`useDictation`, `configureDictation`),
//! `DictationBubble.tsx`, `DictationIsland.tsx` and `DictationButton.tsx`, with
//! the cloud engine left out (see [`engine`]).
//!
//! Ctrl+E toggles one session. The words go to the composer of whichever window
//! is in front — the companion's or the main window's — and to the clipboard when
//! neither is focused, so a transcript is never lost. The bubble window shows the
//! state and, unlike the React island, the live words as they arrive.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

pub mod bubble;
pub mod copy;
pub mod engine;
pub mod settings;

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, Context, Entity, WeakEntity};

use crate::companion::window as floating;
use crate::sound::{DictationCue, SoundEngine};
use crate::ui::root::Root;

use self::copy::{no_model_message, IslandPhase};
use self::engine::{model_status, DictationEngine, DictationEvent, Owner};
use self::settings::{play, DictationSettings};

/// How long the bubble lingers after a session ends, so the finish cue is seen
/// (`BUBBLE_LINGER`).
pub const BUBBLE_LINGER: Duration = Duration::from_millis(2_400);
/// How often the clock ticks for the elapsed timer and the linger.
const TICK: Duration = Duration::from_millis(200);

/// The bubble's two sizes (`DICTATION_COMPACT`, `DICTATION_EXPANDED`).
pub const BUBBLE_COMPACT: (f32, f32) = (260.0, 76.0);
pub const BUBBLE_EXPANDED: (f32, f32) = (460.0, 136.0);
/// How far above the companion's island the bubble sits, in logical pixels.
pub const BUBBLE_GAP: f32 = 132.0;

/// Where dictation is in its own little cycle (`DictationPhase` plus the island's
/// words).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    Listening,
    Processing,
    Done,
    Silent,
    Error,
}

impl Phase {
    pub fn key(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Starting => "starting",
            Phase::Listening => "listening",
            Phase::Processing => "processing",
            Phase::Done => "done",
            Phase::Silent => "silent",
            Phase::Error => "error",
        }
    }

    /// The island's own phase, or `None` while nothing is on screen.
    pub fn island(self) -> Option<IslandPhase> {
        match self {
            Phase::Listening | Phase::Starting => Some(IslandPhase::Listening),
            Phase::Processing => Some(IslandPhase::Processing),
            Phase::Done => Some(IslandPhase::Done),
            Phase::Silent => Some(IslandPhase::Silent),
            Phase::Error => Some(IslandPhase::Error),
            Phase::Idle => None,
        }
    }

    /// Whether a session is running (the shortcut stops it rather than starting
    /// another).
    pub fn active(self) -> bool {
        matches!(self, Phase::Starting | Phase::Listening | Phase::Processing)
    }
}

pub struct Dictation {
    engine: Arc<DictationEngine>,
    pub settings: DictationSettings,
    pub phase: Phase,
    /// The microphone's loudness, for the island's bars.
    pub level: f32,
    pub started_at: Option<Instant>,
    pub stopped_at: Option<Instant>,
    /// Whether this session produced any word at all.
    pub heard: bool,
    /// The model is still loading; audio is queued meanwhile.
    pub warming: bool,
    /// The only text the island ever shows: what went wrong.
    pub error: String,
    /// The live words, shown in the bubble (the brief asks for them; the React
    /// island deliberately shows none).
    pub words: String,
    /// The owner the session belongs to: the companion while it is on screen,
    /// otherwise the main window.
    pub owner: Owner,
    /// When the bubble should hide itself again.
    hide_at: Option<Instant>,
    /// The shortcut is registered for this run.
    pub shortcut: bool,
    /// Where the words go when neither window is in front.
    pub clipboard_note: Option<String>,
    sound: Arc<SoundEngine>,
    main: Option<(WeakEntity<Root>, isize)>,
    main_window: Option<gpui::AnyWindowHandle>,
    companion: Option<(WeakEntity<crate::companion::Companion>, isize)>,
    bubble: Option<(gpui::AnyWindowHandle, isize)>,
}

impl Dictation {
    pub fn new(settings: DictationSettings, sound: Arc<SoundEngine>, cx: &mut Context<Self>) -> Self {
        let (events_tx, events) = async_channel::unbounded();
        let engine = Arc::new(DictationEngine::new(events_tx));
        let this = cx.weak_entity();
        // One reader for the whole app: the engine never touches the UI itself.
        cx.spawn(async move |_, cx: &mut AsyncApp| loop {
            let Ok(event) = events.recv().await else { break };
            if this.upgrade().is_none() {
                break;
            }
            cx.update(|app| {
                if let Some(dictation) = this.upgrade() {
                    dictation.update(app, |dictation, cx| dictation.on_event(event, cx));
                }
            });
        })
        .detach();

        let this = cx.weak_entity();
        cx.spawn(async move |_, cx: &mut AsyncApp| loop {
            cx.background_executor().timer(TICK).await;
            if this.upgrade().is_none() {
                break;
            }
            cx.update(|app| {
                if let Some(dictation) = this.upgrade() {
                    dictation.update(app, |dictation, cx| dictation.tick(cx));
                }
            });
        })
        .detach();

        Self {
            engine,
            settings,
            phase: Phase::Idle,
            level: 0.0,
            started_at: None,
            stopped_at: None,
            heard: false,
            warming: false,
            error: String::new(),
            words: String::new(),
            owner: Owner::Desktop,
            hide_at: None,
            shortcut: false,
            clipboard_note: None,
            sound,
            main: None,
            main_window: None,
            companion: None,
            bubble: None,
        }
    }

    /// The main window the words go to, and its `HWND` for the focus check.
    pub fn attach_main(&mut self, root: WeakEntity<Root>, window: gpui::AnyWindowHandle, hwnd: isize) {
        self.main = Some((root, hwnd));
        self.main_window = Some(window);
    }

    /// The main window's handle, for the bubble's tap.
    pub fn main_handle(&self) -> Option<gpui::AnyWindowHandle> {
        self.main_window
    }

    /// Bring the main window forward (the bubble's tap, show_main_window).
    pub fn show_main_window(&self, cx: &mut App) {
        if let Some(window) = self.main_window {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
    }

    /// The companion window, which owns dictation while it is on screen.
    pub fn attach_companion(&mut self, companion: WeakEntity<crate::companion::Companion>, hwnd: isize) {
        self.companion = Some((companion, hwnd));
    }

    /// The bubble window, created once and hidden until a session runs.
    pub fn attach_bubble(&mut self, window: gpui::AnyWindowHandle, hwnd: isize) {
        self.bubble = Some((window, hwnd));
    }

    pub fn bubble_window(&self) -> Option<isize> {
        self.bubble.map(|(_, hwnd)| hwnd)
    }

    /// Whether the island belongs in the bubble window: dictation runs and the
    /// companion is not on screen to show it.
    pub fn bubble_shows_island(&self) -> bool {
        self.phase != Phase::Idle && !self.companion_visible()
    }

    pub fn companion_visible(&self) -> bool {
        self.companion.as_ref().map(|(_, hwnd)| floating::is_visible(*hwnd)).unwrap_or(false)
    }

    /// Enable or disable dictation for this run: the shortcut follows.
    pub fn configure(&mut self, enabled: bool, hotkeys: Option<&crate::hotkeys::HotkeyManager>) {
        self.settings.enabled = enabled;
        if !enabled {
            self.stop_now();
            self.engine.release();
        }
        if let Some(hotkeys) = hotkeys {
            if enabled {
                if let Err(error) = hotkeys.register(crate::hotkeys::Hotkey::Dictation) {
                    crate::trace::log(format!("dictation: {error}"));
                } else {
                    self.shortcut = true;
                }
            } else if self.shortcut {
                let _ = hotkeys.unregister(crate::hotkeys::Hotkey::Dictation);
                self.shortcut = false;
            }
        }
    }

    /// Load the model in the background, so the first Ctrl+E starts at once
    /// (`configureDictation`).
    pub fn warm(&self) {
        if self.settings.enabled {
            let _ = self.engine.warm(self.settings.model_id());
        }
    }

    /// Ctrl+E: start, or stop what is running.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        if !self.settings.enabled {
            crate::trace::log("dictation: switched off in settings");
            return;
        }
        if self.phase.active() {
            self.stop(cx);
        } else {
            self.start(cx);
        }
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        if let Some(status) = model_status(self.settings.model_id()) {
            if !status.ready {
                // The React app asks to download; this slice has no download
                // flow, so the bubble says what it would say, word for word.
                self.error = no_model_message(&status.label, status.size_bytes);
                self.phase = Phase::Error;
                self.stopped_at = Some(Instant::now());
                self.hide_at = Some(Instant::now() + BUBBLE_LINGER * 3);
                play(&self.sound, &self.settings, DictationCue::Error);
                self.show_bubble(cx);
                cx.notify();
                return;
            }
        } else {
            self.error = "The chosen speech model is not in the catalogue.".to_owned();
            self.phase = Phase::Error;
            self.hide_at = Some(Instant::now() + BUBBLE_LINGER * 3);
            cx.notify();
            return;
        }

        self.owner = if self.companion_visible() { Owner::Companion } else { Owner::Desktop };
        self.phase = Phase::Starting;
        self.heard = false;
        self.warming = false;
        self.error.clear();
        self.words.clear();
        self.clipboard_note = None;
        self.started_at = Some(Instant::now());
        self.stopped_at = None;
        self.hide_at = None;
        let model_id = self.settings.model_id().to_owned();
        if let Err(error) = self.engine.start(&model_id, self.owner) {
            self.phase = Phase::Error;
            self.error = if copy::mentions_missing_model(&error) {
                model_status(&model_id)
                    .map(|status| no_model_message(&status.label, status.size_bytes))
                    .unwrap_or_else(|| copy::friendly_error(&error))
            } else {
                copy::friendly_error(&error)
            };
            self.hide_at = Some(Instant::now() + BUBBLE_LINGER * 3);
            play(&self.sound, &self.settings, DictationCue::Error);
        }
        self.show_bubble(cx);
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.phase.active() {
            return;
        }
        self.phase = Phase::Processing;
        if let Err(error) = self.engine.stop(self.owner) {
            crate::trace::log(format!("dictation: stop failed: {error}"));
        }
        cx.notify();
    }

    /// Escape during a session: drop it and keep nothing.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if !self.phase.active() {
            return;
        }
        self.engine.cancel(self.owner);
        self.stop_now();
        cx.notify();
    }

    fn stop_now(&mut self) {
        self.phase = Phase::Idle;
        self.warming = false;
        self.level = 0.0;
        self.words.clear();
        self.started_at = None;
        self.stopped_at = None;
        self.hide_at = None;
    }

    fn on_event(&mut self, event: DictationEvent, cx: &mut Context<Self>) {
        match event {
            DictationEvent::Ready => {
                self.phase = Phase::Listening;
                self.heard = false;
                self.error.clear();
                self.words.clear();
                self.warming = false;
                self.started_at = Some(Instant::now());
                self.stopped_at = None;
                play(&self.sound, &self.settings, DictationCue::RecordStart);
                self.show_bubble(cx);
            }
            DictationEvent::Loading => self.warming = true,
            DictationEvent::Loaded => self.warming = false,
            DictationEvent::Partial(text) => {
                if !text.trim().is_empty() {
                    self.heard = true;
                    self.words = text;
                }
            }
            DictationEvent::Final(text) => {
                if !text.trim().is_empty() {
                    self.heard = true;
                    self.deliver(text.trim(), cx);
                    self.words.clear();
                }
            }
            DictationEvent::Processing => {
                self.phase = Phase::Processing;
                self.stopped_at = Some(Instant::now());
                play(&self.sound, &self.settings, DictationCue::ProcessingStart);
            }
            DictationEvent::Stopped => {
                self.warming = false;
                let stopped = *self.stopped_at.get_or_insert_with(Instant::now);
                self.stopped_at = Some(stopped);
                self.phase = if self.heard { Phase::Done } else { Phase::Silent };
                self.hide_at = Some(Instant::now() + BUBBLE_LINGER);
                play(&self.sound, &self.settings, DictationCue::ProcessingFinish);
            }
            DictationEvent::Error(text) => {
                self.warming = false;
                self.phase = Phase::Error;
                self.error = if copy::mentions_missing_model(&text) {
                    model_status(self.settings.model_id())
                        .map(|status| no_model_message(&status.label, status.size_bytes))
                        .unwrap_or_else(|| copy::friendly_error(&text))
                } else {
                    copy::friendly_error(&text)
                };
                self.stopped_at = Some(Instant::now());
                self.hide_at = Some(Instant::now() + BUBBLE_LINGER * 3);
                play(&self.sound, &self.settings, DictationCue::Error);
                self.show_bubble(cx);
            }
            DictationEvent::Level(level) => self.level = level,
        }
        cx.notify();
    }

    /// The clock: the elapsed timer redraws once a second, and the bubble hides
    /// itself after the linger.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if let Some(hide_at) = self.hide_at {
            if Instant::now() >= hide_at {
                self.stop_now();
                self.hide_bubble(cx);
                cx.notify();
                return;
            }
        }
        if self.phase.active() {
            cx.notify();
        }
    }

    /// Put a finished transcript where the owner is looking.
    fn deliver(&mut self, text: &str, cx: &mut Context<Self>) {
        let foreground = floating::foreground_hwnd();
        let companion_in_front = self.companion.as_ref().map(|(_, hwnd)| *hwnd == foreground).unwrap_or(false);
        let main_in_front = self.main.as_ref().map(|(_, hwnd)| *hwnd == foreground).unwrap_or(false);

        if companion_in_front {
            if let Some((companion, _)) = self.companion.clone() {
                if let Some(companion) = companion.upgrade() {
                    companion.update(cx, |companion, cx| companion.append_dictated(text, cx));
                    self.clipboard_note = None;
                    return;
                }
            }
        }
        if main_in_front || self.companion.is_none() {
            if let Some((root, _)) = self.main.clone() {
                if let Some(root) = root.upgrade() {
                    let composer = root.read(cx).composer.clone();
                    append_text(&composer, text, cx);
                    self.clipboard_note = None;
                    return;
                }
            }
        }
        // Neither window is in front: keep the words rather than lose them.
        if copy_to_clipboard(text) {
            self.clipboard_note = Some("Copied to the clipboard.".to_owned());
            crate::trace::log("dictation: neither window was in front, so the words went to the clipboard");
        } else {
            self.clipboard_note = Some("The clipboard could not be written.".to_owned());
        }
    }

    fn show_bubble(&mut self, _cx: &mut Context<Self>) {
        if let Some((_, hwnd)) = self.bubble {
            floating::show(_cx.background_executor(), hwnd);
            let expanded = self.phase == Phase::Error || !self.words.is_empty();
            floating::resize_around_bottom_centre(_cx.background_executor(), hwnd, self.preferred_size(expanded));
        }
    }

    fn hide_bubble(&mut self, cx: &mut Context<Self>) {
        if let Some((_, hwnd)) = self.bubble {
            floating::hide(cx.background_executor(), hwnd);
        }
    }

    /// The bubble grows only to explain an error, or to show the live words.
    pub fn preferred_size(&self, expanded: bool) -> (f32, f32) {
        if expanded {
            BUBBLE_EXPANDED
        } else {
            BUBBLE_COMPACT
        }
    }

    /// The status line a companion or a status bar shows.
    pub fn status_line(&self) -> Option<&'static str> {
        self.phase.island().map(|phase| phase.label(self.warming))
    }

    /// How long this session has run, for the island's timer; it freezes when
    /// listening stops, as `Elapsed` does in `DictationIsland`.
    pub fn display_seconds(&self) -> u64 {
        match (self.started_at, self.stopped_at) {
            (Some(start), Some(end)) => end.saturating_duration_since(start).as_secs(),
            (Some(start), None) => start.elapsed().as_secs(),
            _ => 0,
        }
    }

    /// The transcript a view could show; the bubble is the only one that does.
    pub fn live_words(&self) -> &str {
        &self.words
    }
}

/// Append dictation text to a text input, the way the React app does:
/// `current ? current + " " + text : text`.
pub fn append_text(input: &Entity<crate::ui::text_input::TextInput>, text: &str, cx: &mut App) {
    input.update(cx, |input, cx| {
        let current = input.text().trim_end().to_owned();
        if current.is_empty() {
            input.set_text(text.to_owned(), cx);
        } else {
            input.set_text(format!("{current} {text}"), cx);
        }
    });
}

fn copy_to_clipboard(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text.to_owned()))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_phases_map_to_the_islands_own_words() {
        assert_eq!(Phase::Idle.island(), None);
        assert_eq!(Phase::Listening.island(), Some(IslandPhase::Listening));
        assert_eq!(Phase::Processing.island(), Some(IslandPhase::Processing));
        assert_eq!(Phase::Done.island(), Some(IslandPhase::Done));
        assert_eq!(Phase::Error.island(), Some(IslandPhase::Error));
        assert!(Phase::Starting.active() && Phase::Listening.active() && Phase::Processing.active());
        assert!(!Phase::Done.active() && !Phase::Idle.active());
    }

    #[test]
    fn the_bubble_grows_only_for_an_error() {
        let bubble = Dictation {
            engine: Arc::new(DictationEngine::new(async_channel::unbounded().0)),
            settings: DictationSettings::default(),
            phase: Phase::Idle,
            level: 0.0,
            started_at: None,
            stopped_at: None,
            heard: false,
            warming: false,
            error: String::new(),
            words: String::new(),
            owner: Owner::Desktop,
            hide_at: None,
            shortcut: false,
            clipboard_note: None,
            sound: Arc::new(SoundEngine::new()),
            main: None,
            main_window: None,
            companion: None,
            bubble: None,
        };
        assert_eq!(bubble.preferred_size(false), BUBBLE_COMPACT);
        assert_eq!(bubble.preferred_size(true), BUBBLE_EXPANDED);
        assert!(BUBBLE_LINGER >= Duration::from_millis(2_000), "the finish cue stays visible");
    }
}
