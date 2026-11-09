//! Dictation settings, in the native settings file under `dictation`.
//!
//! The React app keeps the model id and the on/off switch in the daemon-held
//! settings (`speech.enabled`, `speech.model`) and the shortcut is registered
//! while dictation is on. Slice 6 builds the Settings rows; this slice reads and
//! writes the same two values plus the one thing the React app has no setting
//! for: the model folder, which this preview never downloads into.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use serde::{Deserialize, Serialize};

use crate::sound;

/// The model the installed app offers first (`DEFAULT_SPEECH_MODEL_ID`).
pub const DEFAULT_SPEECH_MODEL_ID: &str = "parakeet-tdt-0.6b-v3-int8";

/// `speechEnabled` in the React app: dictation is on unless the owner turned it
/// off. The shortcut is only taken while it is on.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DictationSettings {
    pub enabled: bool,
    pub model_id: String,
    /// The dictation island's own cues (`setDictationSoundsEnabled`, on by
    /// default in the React app).
    pub sounds: bool,
    /// Slice 5 has no download flow: a model the installed app already fetched is
    /// reused, and nothing is fetched here. Slice 6's Settings page owns that.
    pub reuse_installed_model: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            model_id: DEFAULT_SPEECH_MODEL_ID.to_owned(),
            sounds: true,
            reuse_installed_model: true,
        }
    }
}

impl DictationSettings {
    pub fn model_id(&self) -> &str {
        let id = self.model_id.trim();
        if id.is_empty() {
            DEFAULT_SPEECH_MODEL_ID
        } else {
            id
        }
    }

    /// The cue set dictation uses: its own table, quieter than the companion's.
    pub fn cue_enabled(&self) -> bool {
        self.sounds
    }

    /// Record which model a session actually ran, so the switch survives a
    /// settings file written by hand.
    pub fn set_model(&mut self, model_id: &str) {
        if !model_id.trim().is_empty() {
            self.model_id = model_id.trim().to_owned();
        }
    }
}

/// The companion's master volume, which the dictation cues ignore: they carry
/// their own gains, exactly as `dictationSound.ts` does (no master gain).
pub fn dictation_gain() -> f32 {
    1.0
}

/// Play one cue if the engine is on. Kept here so the window code never reaches
/// into the sound module's internals.
pub fn play(sound: &sound::SoundEngine, settings: &DictationSettings, cue: sound::DictationCue) {
    if settings.cue_enabled() {
        sound.play_dictation(cue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_is_on_by_default_on_the_installed_apps_first_model() {
        let settings = DictationSettings::default();
        assert!(settings.enabled, "speechEnabled defaults to on");
        assert!(settings.sounds, "the dictation cues are on by default in the React app");
        assert_eq!(settings.model_id(), DEFAULT_SPEECH_MODEL_ID);
        assert!(settings.reuse_installed_model);
    }

    #[test]
    fn an_empty_model_id_falls_back_to_the_default() {
        let mut settings = DictationSettings { model_id: "   ".into(), ..DictationSettings::default() };
        assert_eq!(settings.model_id(), DEFAULT_SPEECH_MODEL_ID);
        settings.set_model("parakeet-tdt-0.6b-v2-int8");
        assert_eq!(settings.model_id(), "parakeet-tdt-0.6b-v2-int8");
        settings.set_model("  ");
        assert_eq!(settings.model_id(), "parakeet-tdt-0.6b-v2-int8", "an empty write changes nothing");
    }

    #[test]
    fn the_settings_round_trip_through_json() {
        let settings = DictationSettings { enabled: false, ..DictationSettings::default() };
        let text = serde_json::to_string(&settings).unwrap();
        assert!(text.contains("\"enabled\":false"), "{text}");
        let back: DictationSettings = serde_json::from_str(&text).unwrap();
        assert!(!back.enabled);
    }
}
