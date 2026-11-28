//! Where this host keeps Retinue's files (17 November 2026, multi-device piece 8).
//!
//! The daemon used to have one answer for every operating system: the database
//! went to `%LOCALAPPDATA%\Retinue`, and the temp folder when that variable was
//! missing. On a headless Linux host there is no `LOCALAPPDATA`, so a daemon
//! started by systemd with no override would have written to `/tmp` and lost
//! everything at the next reboot (`docs/research/multi-device/04-linux-headless.md`,
//! section 3.2). This module is the one place that answers where data lives, so
//! the daemon and the console client agree:
//!
//! * `RETINUE_DB_PATH` always wins (the shell sets it in development; the
//!   systemd unit may set it).
//! * Windows: `%LOCALAPPDATA%\Retinue\retinue.db`, as before.
//! * Unix: `$XDG_DATA_HOME/retinue/retinue.db`, else
//!   `~/.local/share/retinue/retinue.db`, else the temp folder.
//!
//! Everything else the daemon keeps (logs in `logs/`, endpoint files, managed
//! agent copies, secrets in owner-only files) is inside this one folder, so a
//! Linux desktop and its sidecar agree on a path when both derive it from here.
//! `$XDG_STATE_HOME` is deliberately not used for the logs: a second root would
//! split the daemon's files, and the Desktop's own log reader (`retinue_data_dir`
//! in `src-tauri`) already expects the data folder on every platform.
//!
//! The rule is a pure function over a lookup closure, so both platforms' rules
//! are tested on whichever platform runs the tests.

use std::ffi::OsString;
use std::path::PathBuf;

/// `true` when the desktop shell did not start this daemon: the case a headless
/// host is in. The shell always passes `RETINUE_PARENT_PID` (see `run`);
/// `RETINUE_HEADLESS=1` says so explicitly, which is what the systemd unit sets.
pub(crate) fn headless() -> bool {
    if std::env::var_os("RETINUE_HEADLESS").is_some_and(|value| !value.is_empty() && value != "0") {
        return true;
    }
    std::env::var_os("RETINUE_PARENT_PID").is_none()
}

/// The database path when `RETINUE_DB_PATH` is not set.
fn default_db_path() -> PathBuf {
    let temp = std::env::temp_dir();
    default_db_path_from(|name| std::env::var_os(name), cfg!(windows), temp)
}

/// The rule above, separated from the environment it reads.
fn default_db_path_from(lookup: impl Fn(&str) -> Option<OsString>, windows: bool, temp: PathBuf) -> PathBuf {
    let present = |name: &str| lookup(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    if windows {
        if let Some(local) = present("LOCALAPPDATA") {
            return local.join("Retinue").join("retinue.db");
        }
    } else {
        if let Some(data) = present("XDG_DATA_HOME") {
            return data.join("retinue").join("retinue.db");
        }
        if let Some(home) = present("HOME") {
            return home.join(".local").join("share").join("retinue").join("retinue.db");
        }
    }
    // The last resort on both platforms: a host with neither variable keeps its
    // data where the daemon always has, and the Doctor reports the temp folder.
    temp.join("Retinue").join("retinue.db")
}

/// The database path this process uses. `paths::database()` in the daemon;
/// the console client calls it too, so both find the same data folder.
pub(crate) fn database() -> PathBuf {
    std::env::var_os("RETINUE_DB_PATH").map(PathBuf::from).unwrap_or_else(default_db_path)
}

/// The folder the database lives in: Retinue's data folder.
pub(crate) fn data_folder() -> Option<PathBuf> {
    database().parent().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs.iter().map(|(key, value)| (key.to_string(), OsString::from(value))).collect();
        move |name| map.get(name).cloned()
    }

    fn temp() -> PathBuf {
        PathBuf::from("C:/tmp")
    }

    #[test]
    fn windows_uses_local_app_data_and_falls_back_to_temp() {
        assert_eq!(
            default_db_path_from(env(&[("LOCALAPPDATA", "C:/Users/u/AppData/Local")]), true, temp()),
            PathBuf::from("C:/Users/u/AppData/Local").join("Retinue").join("retinue.db")
        );
        assert_eq!(
            default_db_path_from(env(&[("XDG_DATA_HOME", "/home/u/.local/share")]), true, temp()),
            temp().join("Retinue").join("retinue.db"),
            "a Windows host does not read XDG"
        );
        assert_eq!(default_db_path_from(env(&[("LOCALAPPDATA", "")]), true, temp()), temp().join("Retinue").join("retinue.db"), "an empty value is no value");
    }

    #[test]
    fn unix_follows_xdg_then_home_and_never_writes_to_tmp_when_home_exists() {
        assert_eq!(
            default_db_path_from(env(&[("XDG_DATA_HOME", "/home/u/.local/share")]), false, temp()),
            PathBuf::from("/home/u/.local/share").join("retinue").join("retinue.db")
        );
        assert_eq!(
            default_db_path_from(env(&[("HOME", "/home/u")]), false, temp()),
            PathBuf::from("/home/u/.local/share/retinue/retinue.db")
        );
        assert_eq!(
            default_db_path_from(env(&[("XDG_DATA_HOME", ""), ("HOME", "/home/u")]), false, temp()),
            PathBuf::from("/home/u/.local/share/retinue/retinue.db"),
            "an empty XDG_DATA_HOME is no value"
        );
        assert_eq!(
            default_db_path_from(env(&[("LOCALAPPDATA", "C:/Users/u/AppData/Local")]), false, temp()),
            temp().join("Retinue").join("retinue.db"),
            "a Unix host does not read the Windows variable"
        );
        assert_eq!(default_db_path_from(env(&[]), false, temp()), temp().join("Retinue").join("retinue.db"), "only a host with no HOME at all lands in temp");
    }

    #[test]
    fn the_xdg_default_is_under_the_data_folder_so_a_relative_setting_cannot_escape() {
        // The folder is the database's parent, everywhere: the systemd unit and
        // `retinued pair` both derive it this way.
        let path = default_db_path_from(env(&[("HOME", "/home/u")]), false, temp());
        assert_eq!(path.parent().unwrap(), PathBuf::from("/home/u/.local/share/retinue"));
    }
}
