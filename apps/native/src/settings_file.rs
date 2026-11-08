//! Small JSON settings file the window owns (folded projects today). The design
//! language keeps looks and settings in the daemon; this is preview-only state.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NativeSettings {
    /// Projects the owner folded in the sidebar.
    pub folded_projects: Vec<String>,
}

pub fn read(path: &Path) -> NativeSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn read_folded_projects(path: &Path) -> BTreeSet<String> {
    read(path).folded_projects.into_iter().collect()
}

pub fn write_folded_projects(path: &Path, folded: &BTreeSet<String>) -> std::io::Result<()> {
    let mut settings = read(path);
    settings.folded_projects = folded.iter().cloned().collect();
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".to_owned());
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_settings_file_reads_as_empty() {
        let path = std::env::temp_dir().join("bitbots-native-does-not-exist/settings.json");
        assert!(read_folded_projects(&path).is_empty());
    }

    #[test]
    fn folded_projects_survive_a_round_trip() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-settings-{}", std::process::id()));
        let path = folder.join("native-settings.json");
        let mut folded = BTreeSet::new();
        folded.insert("shop".to_owned());
        folded.insert("site".to_owned());
        write_folded_projects(&path, &folded).expect("write");
        assert_eq!(read_folded_projects(&path), folded);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
