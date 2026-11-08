// Where the native preview keeps its own things. It never shares a live database
// with the installed Bitbots: the daemon runs on `%LOCALAPPDATA%\Bitbots Native`,
// and the installed app's database is only ever read, once, to make a copy.

use std::path::PathBuf;

/// The window title and the product name. Distinct from "Bitbots" so the preview
/// never replaces the installed app.
pub const PRODUCT_NAME: &str = "Bitbots (Rust preview)";

/// The folder under `%LOCALAPPDATA%` this app owns.
pub const APP_DIR_NAME: &str = "Bitbots Native";

/// The folder the installed Tauri app owns (read-only for us: the first-run copy).
pub const INSTALLED_DIR_NAME: &str = "Bitbots";

pub fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// `<local app data>\Bitbots Native`
pub fn native_data_dir() -> PathBuf {
    local_app_data().join(APP_DIR_NAME)
}

/// The database this app's daemon opens (`BLOBLEX_DB_PATH`).
pub fn native_db_path() -> PathBuf {
    native_data_dir().join("bloblex.db")
}

/// The installed Bitbots database, the one the first-run prompt offers to copy.
pub fn installed_db_path() -> PathBuf {
    local_app_data().join(INSTALLED_DIR_NAME).join("bloblex.db")
}

/// Small settings file the window owns (folded projects today).
pub fn settings_file_path() -> PathBuf {
    native_data_dir().join("native-settings.json")
}

/// Create the app's own folder. Idempotent.
pub fn ensure_native_data_dir() -> std::io::Result<PathBuf> {
    let dir = native_data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_database_lives_in_its_own_folder() {
        let db = native_db_path();
        assert!(db.starts_with(native_data_dir()));
        assert_eq!(db.file_name().unwrap(), "bloblex.db");
        assert_ne!(db, installed_db_path(), "the preview must not open the installed database");
    }
}
