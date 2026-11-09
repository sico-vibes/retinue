//! The companion's own settings, in the native settings file under
//! `companion` (`companion.soundVolume` and `companion.focusMode` in the React
//! app's daemon-held settings).
//!
//! The brief keeps settings UI out of this slice: the struct exists, with the
//! React app's defaults, and slice 6 builds the switches that write it.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::companion::positions::CompanionPositions;

/// `DEFAULT_SOUND_VOLUME` in `src/blob/soundVolume.ts`.
pub const DEFAULT_SOUND_VOLUME: f32 = 0.8;

/// A saved volume is a number from 0 to 1; anything else is the default.
pub fn normalize_sound_volume(value: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, 1.0) } else { DEFAULT_SOUND_VOLUME }
}

/// One window's size, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSize {
    pub width: f32,
    pub height: f32,
}

impl SavedSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompanionSettings {
    /// Shown at the bottom centre of the work area the first time.
    pub visible: bool,
    /// Whether the first show plays the welcome greeting.
    pub greet_on_launch: bool,
    /// `companion.focusMode`: peeks and their cue are off, approvals still show.
    pub focus_mode: bool,
    /// Sounds are off by default, exactly as `setCompanionSoundsEnabled(false)`.
    pub sounds: bool,
    /// 0 to 1; the master gain is `MASTER_VOLUME * volume / DEFAULT_SOUND_VOLUME`.
    pub sound_volume: f32,
    /// Where the companion was left, per display name (at most 8, as
    /// `companionPositions.ts` allows).
    pub positions: CompanionPositions,
    /// The expanded size the owner last dragged to, if any.
    pub size: Option<SavedSize>,
    /// The large preset ("beside another app") is on.
    pub maximised: bool,
}

impl Default for CompanionSettings {
    fn default() -> Self {
        Self {
            visible: true,
            greet_on_launch: true,
            focus_mode: false,
            sounds: false,
            sound_volume: DEFAULT_SOUND_VOLUME,
            positions: CompanionPositions::default(),
            size: None,
            maximised: false,
        }
    }
}

impl CompanionSettings {
    pub fn volume(&self) -> f32 {
        normalize_sound_volume(self.sound_volume)
    }

    /// A saved size, if it is two positive numbers (`parse_companion_size`).
    pub fn saved_size(&self) -> Option<(f32, f32)> {
        let size = self.size?;
        (size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0)
            .then_some((size.width, size.height))
    }

    pub fn positions(&self) -> &BTreeMap<String, crate::companion::positions::StoredPoint> {
        &self.positions.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_opens_the_companion_with_sounds_off() {
        let settings = CompanionSettings::default();
        assert!(settings.visible, "the companion opens by itself, so the shortcut has something to hide");
        assert!(!settings.sounds, "the React app ships sounds off");
        assert!(!settings.focus_mode);
        assert!((settings.volume() - 0.8).abs() < f32::EPSILON);
        assert!(settings.saved_size().is_none());
    }

    #[test]
    fn a_volume_outside_zero_and_one_reads_as_its_bounds() {
        assert_eq!(normalize_sound_volume(-0.5), 0.0);
        assert_eq!(normalize_sound_volume(1.7), 1.0);
        assert_eq!(normalize_sound_volume(f32::NAN), DEFAULT_SOUND_VOLUME);
    }

    #[test]
    fn a_saved_size_must_be_two_positive_numbers() {
        let mut settings = CompanionSettings { size: Some(SavedSize::new(700.0, 480.0)), ..CompanionSettings::default() };
        assert_eq!(settings.saved_size(), Some((700.0, 480.0)));
        for bad in [(0.0, 400.0), (-1.0, 400.0), (f32::NAN, 400.0)] {
            settings.size = Some(SavedSize::new(bad.0, bad.1));
            assert_eq!(settings.saved_size(), None, "{bad:?}");
        }
    }

    #[test]
    fn the_settings_round_trip_through_json_under_their_own_key() {
        let settings = CompanionSettings {
            sounds: true,
            sound_volume: 0.35,
            size: Some(SavedSize::new(700.0, 480.0)),
            ..CompanionSettings::default()
        };
        let text = serde_json::to_string(&settings).unwrap();
        assert!(text.contains("\"sounds\":true"), "{text}");
        let back: CompanionSettings = serde_json::from_str(&text).unwrap();
        assert!(back.sounds);
        assert_eq!(back.saved_size(), Some((700.0, 480.0)));
        // An older file with no companion block reads as the defaults.
        let empty: CompanionSettings = serde_json::from_str("{}").unwrap();
        assert!(!empty.focus_mode);
    }
}
