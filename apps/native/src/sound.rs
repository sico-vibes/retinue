//! The companion's sounds and the dictation cues.
//!
//! Ported from `apps/desktop/src/blob/soundCues.ts` (the tone table, the master
//! volume and the attack/decay envelope), `soundVolume.ts` (the 0–1 volume with
//! 0.8 as the default) and `ui/dictationSound.ts`. The React app synthesises
//! every cue with Web Audio oscillators and ships no audio files; this does the
//! same with `cpal` and a hand-written oscillator, so nothing is bundled and the
//! same envelopes come out.
//!
//! Sounds are **off by default**, as in the React app. Nothing here logs a word
//! of what was played beyond the cue's own name.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::companion::settings::{normalize_sound_volume, DEFAULT_SOUND_VOLUME};
use crate::trace;

/// Overall loudness at the default volume (`MASTER_VOLUME`).
pub const MASTER_VOLUME: f32 = 0.32;
/// The floor Web Audio's exponential ramps need (`0.0001`).
pub const SILENCE: f32 = 0.0001;
/// The longest a cue may ring before the engine stops waiting for it, seconds.
const MAX_CUE_SECONDS: f32 = 6.0;

/// One of the companion's moments (`CompanionCue`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompanionCue {
    Peek,
    Open,
    Close,
    Hover,
    Blip,
    Tick,
    Send,
    Pop,
    Poke,
    Annoyed,
    Dizzy,
    Love,
    Welcome,
    Work,
    Finish,
    Error,
    Approval,
    Question,
    Approve,
    Gulp,
    Sleep,
}

impl CompanionCue {
    /// Every cue, for the settings preview and the tests.
    pub const ALL: [CompanionCue; 21] = [
        CompanionCue::Peek,
        CompanionCue::Open,
        CompanionCue::Close,
        CompanionCue::Hover,
        CompanionCue::Blip,
        CompanionCue::Tick,
        CompanionCue::Send,
        CompanionCue::Pop,
        CompanionCue::Poke,
        CompanionCue::Annoyed,
        CompanionCue::Dizzy,
        CompanionCue::Love,
        CompanionCue::Welcome,
        CompanionCue::Work,
        CompanionCue::Finish,
        CompanionCue::Error,
        CompanionCue::Approval,
        CompanionCue::Question,
        CompanionCue::Approve,
        CompanionCue::Gulp,
        CompanionCue::Sleep,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CompanionCue::Peek => "peek",
            CompanionCue::Open => "open",
            CompanionCue::Close => "close",
            CompanionCue::Hover => "hover",
            CompanionCue::Blip => "blip",
            CompanionCue::Tick => "tick",
            CompanionCue::Send => "send",
            CompanionCue::Pop => "pop",
            CompanionCue::Poke => "poke",
            CompanionCue::Annoyed => "annoyed",
            CompanionCue::Dizzy => "dizzy",
            CompanionCue::Love => "love",
            CompanionCue::Welcome => "welcome",
            CompanionCue::Work => "work",
            CompanionCue::Finish => "finish",
            CompanionCue::Error => "error",
            CompanionCue::Approval => "approval",
            CompanionCue::Question => "question",
            CompanionCue::Approve => "approve",
            CompanionCue::Gulp => "gulp",
            CompanionCue::Sleep => "sleep",
        }
    }

    /// `completion` is an earlier name for `finish`, kept so a caller that still
    /// uses it plays the right sound.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = if name == "completion" { "finish" } else { name };
        CompanionCue::ALL.into_iter().find(|cue| cue.name() == name)
    }
}

/// The three band-limited shapes Web Audio's `OscillatorType` gives us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
}

impl Wave {
    pub fn from_name(name: &str) -> Wave {
        match name {
            "triangle" => Wave::Triangle,
            "square" => Wave::Square,
            _ => Wave::Sine,
        }
    }

    /// One period of the waveform at `phase` radians.
    fn sample(self, phase: f32) -> f32 {
        match self {
            Wave::Sine => phase.sin(),
            Wave::Triangle => (2.0 / std::f32::consts::PI) * phase.sin().clamp(-1.0, 1.0).asin(),
            Wave::Square => {
                if phase.sin() >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

/// One voice: a pitch that may glide, under a short attack/decay envelope.
#[derive(Clone, Copy, Debug)]
pub struct Tone {
    /// Start, in seconds after the cue begins.
    pub at: f32,
    pub duration: f32,
    pub from: f32,
    /// Glide target; the pitch is held when absent.
    pub to: Option<f32>,
    pub wave: Wave,
    /// Relative loudness before the master volume.
    pub level: f32,
    /// A slow pitch wobble: (rate in Hz, depth as a fraction of the pitch).
    pub wobble: Option<(f32, f32)>,
}

impl Tone {
    const fn held(at: f32, duration: f32, from: f32, wave: Wave, level: f32) -> Self {
        Self { at, duration, from, to: None, wave, level, wobble: None }
    }

    const fn glide(at: f32, duration: f32, from: f32, to: f32, wave: Wave, level: f32) -> Self {
        Self { at, duration, from, to: Some(to), wave, level, wobble: None }
    }

    const fn wobble(mut self, rate: f32, depth: f32) -> Self {
        self.wobble = Some((rate, depth));
        self
    }

    /// The envelope at `seconds` into the tone: a 1/10000th start, an exponential
    /// attack of at most 12 ms to the peak, and an exponential decay back to it
    /// (`playCompanionCue`).
    pub fn envelope(&self, seconds: f32) -> f32 {
        if seconds < 0.0 || seconds > self.duration {
            return 0.0;
        }
        let peak = self.level.max(0.0002);
        let attack = self.duration.min(0.012).min(self.duration / 4.0).max(0.0005);
        if seconds < attack {
            let u = seconds / attack;
            return SILENCE * (peak / SILENCE).powf(u);
        }
        let span = (self.duration - attack).max(0.0005);
        let u = ((seconds - attack) / span).clamp(0.0, 1.0);
        peak * (SILENCE / peak).powf(u)
    }

    /// The pitch at `seconds` into the tone, glides and wobble included.
    pub fn frequency(&self, seconds: f32) -> f32 {
        let u = if self.duration > 0.0 { (seconds / self.duration).clamp(0.0, 1.0) } else { 0.0 };
        let base = match self.to {
            Some(to) => self.from * (to / self.from).powf(u),
            None => self.from,
        };
        match self.wobble {
            Some((rate, depth)) => {
                let wobble = (u * self.duration * rate * std::f32::consts::TAU).sin() * depth;
                (base * (1.0 + wobble)).max(20.0)
            }
            None => base,
        }
    }

    /// When the tone finishes, in seconds after the cue began.
    pub fn end(&self) -> f32 {
        self.at + self.duration
    }
}

/// `bell(at, frequency, duration, level)`: a sine and its quiet octave.
fn bell(at: f32, frequency: f32, duration: f32, level: f32) -> [Tone; 2] {
    [
        Tone::held(at, duration, frequency, Wave::Sine, level),
        Tone::held(at, duration * 0.6, frequency * 2.0, Wave::Sine, level * 0.22),
    ]
}

/// The companion cue table (`CUES` in `soundCues.ts`), tone for tone.
pub fn companion_tones(cue: CompanionCue) -> Vec<Tone> {
    match cue {
        CompanionCue::Peek => vec![Tone::glide(0.0, 0.09, 330.0, 720.0, Wave::Sine, 0.7)],
        CompanionCue::Open => {
            let mut tones = vec![Tone::glide(0.0, 0.13, 420.0, 880.0, Wave::Triangle, 0.55)];
            tones.extend(bell(0.07, 1046.5, 0.12, 0.35));
            tones
        }
        CompanionCue::Close => vec![Tone::glide(0.0, 0.12, 820.0, 380.0, Wave::Triangle, 0.5)],
        CompanionCue::Hover => vec![Tone::glide(0.0, 0.07, 990.0, 1120.0, Wave::Sine, 0.28)],
        CompanionCue::Blip => vec![Tone::glide(0.0, 0.055, 1180.0, 1420.0, Wave::Sine, 0.45)],
        CompanionCue::Tick => vec![
            Tone::held(0.0, 0.03, 2100.0, Wave::Square, 0.12),
            Tone::held(0.0, 0.04, 1400.0, Wave::Sine, 0.3),
        ],
        CompanionCue::Send => vec![
            Tone::glide(0.0, 0.15, 520.0, 1250.0, Wave::Sine, 0.6),
            Tone::glide(0.02, 0.12, 780.0, 1560.0, Wave::Triangle, 0.2),
        ],
        CompanionCue::Pop => vec![Tone::glide(0.0, 0.08, 1250.0, 520.0, Wave::Sine, 0.7)],
        CompanionCue::Poke => vec![Tone::glide(0.0, 0.16, 300.0, 190.0, Wave::Sine, 0.75).wobble(28.0, 0.06)],
        CompanionCue::Annoyed => vec![
            Tone::held(0.0, 0.1, 330.0, Wave::Triangle, 0.55),
            Tone::glide(0.11, 0.16, 262.0, 233.0, Wave::Triangle, 0.55),
        ],
        CompanionCue::Dizzy => vec![Tone::glide(0.0, 0.6, 720.0, 300.0, Wave::Sine, 0.6).wobble(9.0, 0.08)],
        CompanionCue::Love => {
            let mut tones = bell(0.0, 784.0, 0.16, 0.6).to_vec();
            tones.extend(bell(0.12, 1046.5, 0.26, 0.6));
            tones
        }
        CompanionCue::Welcome => {
            let mut tones = bell(0.0, 523.25, 0.14, 0.8).to_vec();
            tones.extend(bell(0.1, 659.25, 0.14, 0.8));
            tones.extend(bell(0.2, 783.99, 0.14, 0.8));
            tones.extend(bell(0.3, 1046.5, 0.34, 0.8));
            tones
        }
        CompanionCue::Work => vec![
            Tone::held(0.0, 0.09, 494.0, Wave::Sine, 0.45),
            Tone::held(0.09, 0.12, 587.0, Wave::Sine, 0.45),
        ],
        CompanionCue::Finish => {
            let mut tones = bell(0.0, 659.25, 0.13, 0.8).to_vec();
            tones.extend(bell(0.1, 830.6, 0.13, 0.8));
            tones.extend(bell(0.2, 987.8, 0.36, 0.8));
            tones
        }
        CompanionCue::Error => vec![
            Tone::held(0.0, 0.14, 392.0, Wave::Triangle, 0.6),
            Tone::glide(0.15, 0.24, 311.1, 293.7, Wave::Triangle, 0.6),
        ],
        CompanionCue::Approval => {
            let mut tones = bell(0.0, 880.0, 0.18, 0.75).to_vec();
            tones.extend(bell(0.17, 698.5, 0.3, 0.75));
            tones
        }
        CompanionCue::Question => vec![
            Tone::held(0.0, 0.09, 587.3, Wave::Sine, 0.55),
            Tone::glide(0.09, 0.2, 740.0, 880.0, Wave::Sine, 0.55),
        ],
        CompanionCue::Approve => {
            let mut tones = bell(0.0, 698.5, 0.1, 0.65).to_vec();
            tones.extend(bell(0.08, 1046.5, 0.22, 0.65));
            tones
        }
        CompanionCue::Gulp => vec![
            Tone::glide(0.0, 0.14, 260.0, 120.0, Wave::Sine, 0.8),
            Tone::glide(0.12, 0.07, 900.0, 420.0, Wave::Sine, 0.35),
        ],
        CompanionCue::Sleep => vec![
            Tone::glide(0.0, 0.3, 523.25, 440.0, Wave::Sine, 0.35),
            Tone::glide(0.26, 0.42, 392.0, 349.2, Wave::Sine, 0.3),
        ],
    }
}

/// The dictation cues (`DictationCue` in `dictationSound.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DictationCue {
    RecordStart,
    ProcessingStart,
    ProcessingFinish,
    Error,
}

impl DictationCue {
    pub fn name(self) -> &'static str {
        match self {
            DictationCue::RecordStart => "record-start",
            DictationCue::ProcessingStart => "processing-start",
            DictationCue::ProcessingFinish => "processing-finish",
            DictationCue::Error => "error",
        }
    }
}

/// A dictation tone spec: frequency, duration, delay and gain, in the units the
/// TypeScript table uses.
#[derive(Clone, Copy, Debug)]
struct DictationTone {
    frequency: f32,
    duration_ms: f32,
    delay_ms: f32,
    gain: f32,
}

fn dictation_specs(cue: DictationCue) -> Vec<DictationTone> {
    match cue {
        DictationCue::RecordStart => vec![
            DictationTone { frequency: 523.0, duration_ms: 85.0, delay_ms: 0.0, gain: 0.021 },
            DictationTone { frequency: 659.0, duration_ms: 120.0, delay_ms: 100.0, gain: 0.021 },
        ],
        DictationCue::ProcessingStart => vec![
            DictationTone { frequency: 494.0, duration_ms: 90.0, delay_ms: 0.0, gain: 0.022 },
            DictationTone { frequency: 622.0, duration_ms: 125.0, delay_ms: 100.0, gain: 0.022 },
        ],
        DictationCue::ProcessingFinish => vec![
            DictationTone { frequency: 523.0, duration_ms: 95.0, delay_ms: 0.0, gain: 0.03 },
            DictationTone { frequency: 659.0, duration_ms: 95.0, delay_ms: 120.0, gain: 0.03 },
            DictationTone { frequency: 784.0, duration_ms: 150.0, delay_ms: 240.0, gain: 0.03 },
        ],
        DictationCue::Error => vec![
            DictationTone { frequency: 392.0, duration_ms: 120.0, delay_ms: 0.0, gain: 0.026 },
            DictationTone { frequency: 311.0, duration_ms: 170.0, delay_ms: 140.0, gain: 0.026 },
            DictationTone { frequency: 262.0, duration_ms: 180.0, delay_ms: 310.0, gain: 0.026 },
        ],
    }
}

pub fn dictation_tones(cue: DictationCue) -> Vec<Tone> {
    dictation_specs(cue)
        .into_iter()
        .map(|spec| Tone::held(spec.delay_ms / 1000.0, spec.duration_ms / 1000.0, spec.frequency, Wave::Sine, spec.gain))
        .collect()
}

/// The master gain for a volume setting: the default keeps the loudness the
/// cues always had, 100% is a quarter louder, 0 is silent.
pub fn sound_volume_gain(volume: f32) -> f32 {
    MASTER_VOLUME * (normalize_sound_volume(volume) / DEFAULT_SOUND_VOLUME)
}

/// How long a cue rings, in seconds.
pub fn cue_seconds(tones: &[Tone]) -> f32 {
    tones.iter().map(|tone| tone.end()).fold(0.0, f32::max)
}

/// Render a cue to mono samples. The engine plays through the same maths; this
/// is the version a test can look at.
pub fn render(tones: &[Tone], sample_rate: u32, gain: f32) -> Vec<f32> {
    let seconds = cue_seconds(tones);
    let frames = (seconds * sample_rate as f32).ceil() as usize + 1;
    let mut out = vec![0.0; frames];
    for tone in tones {
        let mut phase = 0.0f32;
        let first = (tone.at * sample_rate as f32).round() as usize;
        let last = ((tone.at + tone.duration) * sample_rate as f32).ceil() as usize;
        for (frame, sample) in out.iter_mut().enumerate().take(last.min(frames)).skip(first) {
            let t = frame as f32 / sample_rate as f32 - tone.at;
            let frequency = tone.frequency(t);
            phase += std::f32::consts::TAU * frequency / sample_rate as f32;
            *sample += tone.wave.sample(phase) * tone.envelope(t) * gain;
        }
    }
    out
}

/// What the audio thread is asked to do.
enum Message {
    Play { tones: Vec<Tone>, gain: f32 },
    Volume(f32),
    Silence,
    Stop,
}

/// The app's handle on the sound engine. Cheap to clone; safe to hold in a view.
pub struct SoundEngine {
    sender: Sender<Message>,
    enabled: Arc<AtomicBool>,
    /// The volume's bits, so the audio callback can read it without a lock.
    volume: Arc<AtomicU32>,
}

impl SoundEngine {
    /// Start the audio thread. It builds no audio stream until something is
    /// actually played, so an install with sounds off costs nothing.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Message>();
        let enabled = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(DEFAULT_SOUND_VOLUME.to_bits()));
        let thread_enabled = Arc::clone(&enabled);
        let thread_volume = Arc::clone(&volume);
        let _ = std::thread::Builder::new()
            .name("bitbots-sound".to_owned())
            .spawn(move || run(receiver, thread_enabled, thread_volume));
        Self { sender, enabled, volume }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Load the preference. Turning sounds off silences what is ringing, exactly
    /// as `setCompanionSoundsEnabled(false)` does.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if !enabled {
            let _ = self.sender.send(Message::Silence);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = normalize_sound_volume(volume);
        self.volume.store(volume.to_bits(), Ordering::Relaxed);
        let _ = self.sender.send(Message::Volume(volume));
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Play one companion cue. False when sounds are off; true when the cue was
    /// handed to the audio thread, including at volume 0 (muted, not switched
    /// off), so a caller never retries.
    pub fn play(&self, cue: CompanionCue) -> bool {
        self.play_tones(companion_tones(cue))
    }

    pub fn play_dictation(&self, cue: DictationCue) -> bool {
        self.play_tones(dictation_tones(cue))
    }

    fn play_tones(&self, tones: Vec<Tone>) -> bool {
        if !self.is_enabled() {
            return false;
        }
        if self.volume() <= 0.0 {
            return true;
        }
        self.sender.send(Message::Play { tones, gain: 1.0 }).is_ok()
    }
}

impl Default for SoundEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SoundEngine {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
    }
}

/// One voice playing: its tone table, when it starts, and the phase of each tone
/// so a glide or a wobble stays continuous across buffers.
struct Voice {
    tones: Vec<Tone>,
    gain: f32,
    /// The frame the cue begins on, in the mixer's own count.
    starts_at: u64,
    phases: Vec<f32>,
}

struct Mixer {
    voices: Vec<Voice>,
    /// Frames rendered so far, so a cue can be scheduled a little ahead.
    frames: u64,
    sample_rate: u32,
    channels: usize,
    /// The owner's master gain (`sound_volume_gain`).
    master: f32,
}

impl Mixer {
    fn new(sample_rate: u32, channels: usize) -> Self {
        Self { voices: Vec::new(), frames: 0, sample_rate, channels, master: MASTER_VOLUME }
    }

    /// Two buffers of lead, so the first samples are not lost to the callback's
    /// own latency.
    fn lead_frames(&self) -> u64 {
        (self.sample_rate as u64 / 50).max(1) * 2
    }

    fn push(&mut self, tones: Vec<Tone>, gain: f32) {
        let phases = vec![0.0; tones.len()];
        let starts_at = self.frames + self.lead_frames();
        self.voices.push(Voice { tones, gain, starts_at, phases });
    }

    fn silence(&mut self) {
        self.voices.clear();
    }

    fn render(&mut self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            *sample = 0.0;
        }
        let channels = self.channels.max(1);
        let frames = out.len() / channels;
        let sample_rate = self.sample_rate as f32;
        for index in 0..frames {
            let frame = self.frames;
            let mut mixed = 0.0f32;
            for voice in self.voices.iter_mut() {
                if frame < voice.starts_at {
                    continue;
                }
                let t = (frame - voice.starts_at) as f32 / sample_rate;
                for (tone_index, tone) in voice.tones.iter().enumerate() {
                    if t < tone.at || t > tone.at + tone.duration {
                        continue;
                    }
                    let local = t - tone.at;
                    let frequency = tone.frequency(local);
                    let phase = &mut voice.phases[tone_index];
                    *phase += std::f32::consts::TAU * frequency / sample_rate;
                    mixed += tone.wave.sample(*phase) * tone.envelope(local) * voice.gain;
                }
            }
            mixed = (mixed * self.master).clamp(-1.0, 1.0);
            for channel in 0..channels {
                out[index * channels + channel] = mixed;
            }
            self.frames += 1;
            if self.frames.is_multiple_of(self.sample_rate as u64 * 5) {
                // Drop voices whose last tone has rung out.
                let now = self.frames;
                self.voices.retain(|voice| {
                    let longest = cue_seconds(&voice.tones).min(MAX_CUE_SECONDS);
                    let age = now.saturating_sub(voice.starts_at) as f32 / sample_rate;
                    age < longest + 0.5
                });
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    mutex.lock().ok()
}

/// The audio thread: build the stream when the first cue arrives, then serve
/// cues until the engine is dropped. The stream is `!Send`, so it never leaves
/// this thread.
fn run(receiver: mpsc::Receiver<Message>, enabled: Arc<AtomicBool>, volume: Arc<AtomicU32>) {
    let mixer = Arc::new(Mutex::new(None::<Mixer>));
    let mut stream: Option<cpal::Stream> = None;
    loop {
        let message = match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match message {
            Message::Play { tones, gain } => {
                if !enabled.load(Ordering::Relaxed) {
                    continue;
                }
                if stream.is_none() {
                    match build_stream(Arc::clone(&mixer)) {
                        Ok(built) => stream = Some(built),
                        Err(error) => {
                            trace::log(format!("sound: no audio output: {error}"));
                            continue;
                        }
                    }
                }
                if let Some(mut guard) = lock(&mixer) {
                    if let Some(mixer) = guard.as_mut() {
                        mixer.master = sound_volume_gain(f32::from_bits(volume.load(Ordering::Relaxed)));
                        mixer.push(tones, gain);
                    }
                }
            }
            Message::Volume(value) => {
                if let Some(mut guard) = lock(&mixer) {
                    if let Some(mixer) = guard.as_mut() {
                        mixer.master = sound_volume_gain(value);
                    }
                }
            }
            Message::Silence => {
                if let Some(mut guard) = lock(&mixer) {
                    if let Some(mixer) = guard.as_mut() {
                        mixer.silence();
                    }
                }
            }
            Message::Stop => break,
        }
    }
    drop(stream);
}

fn build_stream(mixer: Arc<Mutex<Option<Mixer>>>) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(|| "no default output device".to_owned())?;
    let supported = device.default_output_config().map_err(|error| error.to_string())?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;
    if let Some(mut guard) = lock(&mixer) {
        *guard = Some(Mixer::new(sample_rate, channels));
    }

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let data = Arc::clone(&mixer);
            let errors = Arc::clone(&mixer);
            device.build_output_stream(
                &config,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| render_into_f32(&data, out),
                move |error| report_error(&errors, error),
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let data = Arc::clone(&mixer);
            let errors = Arc::clone(&mixer);
            device.build_output_stream(
                &config,
                move |out: &mut [i16], _: &cpal::OutputCallbackInfo| render_into_i16(&data, out),
                move |error| report_error(&errors, error),
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let data = Arc::clone(&mixer);
            let errors = Arc::clone(&mixer);
            device.build_output_stream(
                &config,
                move |out: &mut [u16], _: &cpal::OutputCallbackInfo| render_into_u16(&data, out),
                move |error| report_error(&errors, error),
                None,
            )
        }
        other => return Err(format!("unsupported sample format {other:?}")),
    }
    .map_err(|error| error.to_string())?;

    StreamTrait::play(&stream).map_err(|error| error.to_string())?;
    trace::log(format!("sound: audio output ready ({sample_rate} Hz, {channels} channels)"));
    Ok(stream)
}

fn report_error(_mixer: &Arc<Mutex<Option<Mixer>>>, error: cpal::StreamError) {
    // The device's own message only: no audio, no path, nothing about a cue.
    trace::log(format!("sound: audio stream error: {error}"));
}

/// Fill `scratch` with the mixer's own float samples. The callback never waits
/// on another thread: a mixer that is busy is silence.
fn fill(mixer: &Arc<Mutex<Option<Mixer>>>, scratch: &mut Vec<f32>, len: usize) {
    scratch.clear();
    scratch.resize(len, 0.0);
    match mixer.try_lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(mixer) => mixer.render(scratch),
            None => scratch.iter_mut().for_each(|sample| *sample = 0.0),
        },
        Err(_) => scratch.iter_mut().for_each(|sample| *sample = 0.0),
    }
}

fn render_into_f32(mixer: &Arc<Mutex<Option<Mixer>>>, out: &mut [f32]) {
    let mut scratch = Vec::new();
    fill(mixer, &mut scratch, out.len());
    out.copy_from_slice(&scratch);
}

fn render_into_i16(mixer: &Arc<Mutex<Option<Mixer>>>, out: &mut [i16]) {
    let mut scratch = Vec::new();
    fill(mixer, &mut scratch, out.len());
    for (target, value) in out.iter_mut().zip(scratch.iter()) {
        *target = (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
    }
}

fn render_into_u16(mixer: &Arc<Mutex<Option<Mixer>>>, out: &mut [u16]) {
    let mut scratch = Vec::new();
    fill(mixer, &mut scratch, out.len());
    for (target, value) in out.iter_mut().zip(scratch.iter()) {
        *target = ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    #[test]
    fn the_cue_table_matches_the_react_apps_tones() {
        let peek = companion_tones(CompanionCue::Peek);
        assert_eq!(peek.len(), 1);
        assert_eq!((peek[0].from, peek[0].to), (330.0, Some(720.0)));
        assert!((peek[0].duration - 0.09).abs() < 1e-6);
        assert_eq!(peek[0].wave, Wave::Sine);

        // `open` is one glide plus a bell, and a bell is a sine and its octave.
        let open = companion_tones(CompanionCue::Open);
        assert_eq!(open.len(), 3);
        assert_eq!(open[0].wave, Wave::Triangle);
        assert_eq!(open[1].from, 1046.5);
        assert_eq!(open[2].from, 2093.0);
        assert!((open[2].level - 0.35 * 0.22).abs() < 1e-6);
        assert!((open[2].duration - 0.12 * 0.6).abs() < 1e-6);

        let finish = companion_tones(CompanionCue::Finish);
        assert_eq!(finish.len(), 6);
        assert!((cue_seconds(&finish) - 0.56).abs() < 1e-6);
        assert!(companion_tones(CompanionCue::Welcome).len() == 8);

        // An earlier name still plays the right cue.
        assert_eq!(CompanionCue::from_name("completion"), Some(CompanionCue::Finish));
        assert_eq!(CompanionCue::from_name("finish"), Some(CompanionCue::Finish));
        assert_eq!(CompanionCue::from_name("nope"), None);
    }

    #[test]
    fn every_cue_is_short_and_every_tone_has_a_level() {
        for cue in CompanionCue::ALL {
            let tones = companion_tones(cue);
            assert!(!tones.is_empty(), "{}", cue.name());
            let seconds = cue_seconds(&tones);
            assert!(seconds > 0.0 && seconds <= MAX_CUE_SECONDS, "{} runs {seconds}s", cue.name());
            for tone in &tones {
                assert!(tone.level > 0.0 && tone.level <= 1.0, "{}", cue.name());
                assert!(tone.duration > 0.0);
                assert!(tone.from >= 20.0);
            }
        }
    }

    #[test]
    fn the_envelope_starts_quiet_peaks_past_the_attack_and_decays_to_nothing() {
        let tone = Tone::glide(0.0, 0.15, 520.0, 1250.0, Wave::Sine, 0.6);
        assert!((tone.envelope(0.0) - SILENCE).abs() < 1e-6);
        let attack = tone.duration.min(0.012).min(tone.duration / 4.0);
        let peak = tone.envelope(attack);
        assert!((peak - 0.6).abs() < 1e-3, "the peak is the tone's level: {peak}");
        assert!(tone.envelope(attack / 2.0) < peak, "the attack rises");
        assert!(tone.envelope(tone.duration - 0.001) < 0.01, "the tail decays");
        assert_eq!(tone.envelope(-0.1), 0.0);
        assert_eq!(tone.envelope(tone.duration + 0.1), 0.0);
    }

    #[test]
    fn a_glide_runs_from_its_start_pitch_to_its_end_pitch() {
        let tone = Tone::glide(0.0, 0.2, 400.0, 800.0, Wave::Sine, 0.5);
        assert!((tone.frequency(0.0) - 400.0).abs() < 0.01);
        assert!((tone.frequency(0.2) - 800.0).abs() < 0.01);
        let held = Tone::held(0.0, 0.2, 440.0, Wave::Sine, 0.5);
        assert_eq!(held.frequency(0.1), 440.0);
        // A wobble never takes a tone below hearing (20 Hz).
        let wobbling = Tone::glide(0.0, 0.6, 720.0, 300.0, Wave::Sine, 0.6).wobble(9.0, 0.08);
        for step in 0..40 {
            let frequency = wobbling.frequency(step as f32 * 0.015);
            assert!(frequency >= 20.0, "{frequency}");
        }
    }

    #[test]
    fn the_master_gain_follows_the_volume_setting() {
        assert!((sound_volume_gain(0.8) - MASTER_VOLUME).abs() < 1e-6, "the default keeps the loudness");
        assert!((sound_volume_gain(1.0) - MASTER_VOLUME * 1.25).abs() < 1e-6, "100% is a quarter louder");
        assert!((sound_volume_gain(0.4) - MASTER_VOLUME * 0.5).abs() < 1e-6);
        assert_eq!(sound_volume_gain(0.0), 0.0);
        assert!((sound_volume_gain(f32::NAN) - MASTER_VOLUME).abs() < 1e-6, "an unreadable volume is the default");
    }

    #[test]
    fn a_rendered_cue_is_audible_inside_its_window_and_silent_outside_it() {
        let tone = Tone::glide(0.0, 0.09, 330.0, 720.0, Wave::Sine, 0.7);
        let samples = render(&[tone], RATE, sound_volume_gain(0.8));
        let frames = (0.09 * RATE as f32).ceil() as usize + 1;
        assert_eq!(samples.len(), frames);
        let peak = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.01, "a cue is audible: peak {peak}");
        assert!(peak < MASTER_VOLUME, "and quiet: peak {peak}");
        // The first and last samples are near silence, as the envelope says.
        assert!(samples[0].abs() < 0.01);
        assert!(samples[samples.len() - 1].abs() < 0.01);
    }

    #[test]
    fn the_dictation_cues_keep_their_own_gains_and_timing() {
        let start = dictation_tones(DictationCue::RecordStart);
        assert_eq!(start.len(), 2);
        assert_eq!((start[0].from, start[0].level), (523.0, 0.021));
        assert!((start[1].at - 0.1).abs() < 1e-6);
        assert!((start[1].duration - 0.12).abs() < 1e-6);
        let finish = dictation_tones(DictationCue::ProcessingFinish);
        assert_eq!(finish.len(), 3);
        assert!((finish[2].at - 0.24).abs() < 1e-6);
        assert_eq!(finish[2].from, 784.0);
        assert_eq!(DictationCue::Error.name(), "error");
        assert!((cue_seconds(&dictation_tones(DictationCue::Error)) - 0.49).abs() < 1e-6);
        // These are far quieter than the companion's cues, as in the app.
        assert!(start[0].level < companion_tones(CompanionCue::Peek)[0].level);
    }

    #[test]
    fn a_mixer_schedules_a_cue_a_little_ahead_and_mixes_two_voices() {
        let mut mixer = Mixer::new(RATE, 1);
        mixer.push(vec![Tone::held(0.0, 0.05, 440.0, Wave::Sine, 0.5)], 0.32);
        mixer.push(vec![Tone::held(0.0, 0.05, 660.0, Wave::Sine, 0.5)], 0.32);
        // Render past the lead-in and the cue's own length.
        let mut buffer = vec![0.0f32; (RATE as f32 * 0.2) as usize];
        mixer.render(&mut buffer);
        let peak = buffer.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.01 && peak <= 1.0, "two voices mixed: {peak}");
        assert!(mixer.voices.len() <= 2);
        mixer.silence();
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn sounds_off_means_nothing_is_played_and_zero_volume_is_silent_but_handed_over() {
        let engine = SoundEngine::new();
        assert!(!engine.is_enabled(), "sounds are off by default");
        assert!(!engine.play(CompanionCue::Peek), "off: no cue");
        engine.set_enabled(true);
        assert!(engine.play(CompanionCue::Peek), "on: the cue goes to the audio thread");
        engine.set_volume(0.0);
        assert!(engine.play(CompanionCue::Peek), "volume 0 is muted without switching sounds off");
        engine.set_volume(2.0);
        assert!((engine.volume() - 1.0).abs() < f32::EPSILON, "a volume above 1 reads as 1");
    }
}
