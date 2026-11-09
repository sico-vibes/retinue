//! Small JSON settings file the window owns: the folded projects, the theme and
//! the density. The design language keeps looks and product settings in the
//! daemon; this is preview-only state, so it never travels in a backup.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::theme::{Density, ThemeChoice};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NativeSettings {
    /// Projects the owner folded in the sidebar.
    pub folded_projects: Vec<String>,
    /// `system`, `light` or `dark`.
    pub theme: ThemeChoice,
    /// `compact`, `regular` or `roomy`.
    pub density: Density,
    /// Conversations the owner pinned, in the order they pinned them.
    pub pinned_sessions: Vec<String>,
    /// Whether the sidebar shows the plan-usage line (`experimental.usageBar`).
    pub usage_bar: bool,
    /// Slice 5: the companion's sounds, volume, positions and size. Stored under
    /// `companion`; slice 6 builds the switches that write it.
    pub companion: crate::companion::CompanionSettings,
    /// Slice 5: dictation's on/off switch and model, under `dictation`.
    pub dictation: crate::dictation::settings::DictationSettings,
}

pub fn read(path: &Path) -> NativeSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Projects the owner folded in the sidebar, read once at startup.
#[cfg(test)]
pub fn read_folded_projects(path: &Path) -> BTreeSet<String> {
    read(path).folded_projects.into_iter().collect()
}

pub fn write_folded_projects(path: &Path, folded: &BTreeSet<String>) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.folded_projects = folded.iter().cloned().collect();
    write(path, &settings)
}

/// Save the appearance choice (Settings → Appearance).
pub fn write_appearance(path: &Path, theme: ThemeChoice, density: Density) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.theme = theme;
    settings.density = density;
    write(path, &settings)
}

/// Save the pinned conversations, in their own order.
pub fn write_pinned_sessions(path: &Path, pinned: &[String]) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.pinned_sessions = pinned.to_vec();
    write(path, &settings)
}

/// Save whether the sidebar shows the plan-usage line.
pub fn write_usage_bar(path: &Path, enabled: bool) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.usage_bar = enabled;
    write(path, &settings)
}

/// Save the companion's own settings (its sounds, volume, position and size).
pub fn write_companion(path: &Path, companion: &crate::companion::CompanionSettings) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.companion = companion.clone();
    write(path, &settings)
}

/// Save dictation's own settings (the switch and the model).
#[allow(dead_code)] // Slice 6's Settings page is the first writer.
pub fn write_dictation(path: &Path, dictation: &crate::dictation::settings::DictationSettings) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.dictation = dictation.clone();
    write(path, &settings)
}

fn write(path: &Path, settings: &NativeSettings) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_owned());
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_settings_file_reads_as_empty() {
        let path = std::env::temp_dir().join("retinue-native-does-not-exist/settings.json");
        assert!(read_folded_projects(&path).is_empty());
        let settings = read(&path);
        assert_eq!(settings.theme, ThemeChoice::System, "a new install follows Windows");
        assert_eq!(settings.density, Density::Regular);
    }

    #[test]
    fn folded_projects_survive_a_round_trip() {
        let folder = std::env::temp_dir().join(format!("retinue-native-settings-{}", std::process::id()));
        let path = folder.join("native-settings.json");
        let mut folded = BTreeSet::new();
        folded.insert("shop".to_owned());
        folded.insert("site".to_owned());
        write_folded_projects(&path, &folded).expect("write");
        assert_eq!(read_folded_projects(&path), folded);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_appearance_choice_survives_and_leaves_the_folds_alone() {
        let folder = std::env::temp_dir().join(format!("retinue-native-appearance-{}", std::process::id()));
        let path = folder.join("native-settings.json");
        let mut folded = BTreeSet::new();
        folded.insert("shop".to_owned());
        write_folded_projects(&path, &folded).expect("write");
        write_appearance(&path, ThemeChoice::Light, Density::Compact).expect("write");
        let settings = read(&path);
        assert_eq!(settings.theme, ThemeChoice::Light);
        assert_eq!(settings.density, Density::Compact);
        assert_eq!(read_folded_projects(&path), folded, "a theme change keeps the folds");
        // The file stores the words the Settings page offers.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"light\"") && text.contains("\"compact\""), "{text}");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
