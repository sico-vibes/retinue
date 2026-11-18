//! Phase 5: secrets in the Windows credential store, and one-use secret requests.
//!
//! Section 5: a bot's environment variable can point at a named secret instead
//! of holding plain text. The value lives in Credential Manager under
//! `Retinue/<name>` and is read only at spawn. No RPC returns a value, and the
//! value never reaches the database, an event, a log, a reply, a backup or a
//! diagnostics bundle. A coding agent can read its own environment, so this
//! protects the value at rest, not from the bot.
//!
//! Section 6: a bot may ask the owner for a secret for one run with
//! `request_secret`. The request is held in memory only (never in the database),
//! the tool call waits for the owner, and the answer goes to a private file the
//! bot is told about. The file is deleted when the turn ends, and any leftover
//! is deleted at startup.

use crate::{derr, team, AppState, DispatchError};
use axum::http::StatusCode;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

const NAME_MAX: usize = 64;
/// The credential store's limit for a generic credential, as the plan states it.
pub(crate) const VALUE_MAX: usize = 2_048;
/// A one-use file is not bound by the credential store, so it takes a longer
/// value (a private key, say), but still a bounded one.
const ONE_USE_MAX: usize = 16_384;
const REASON_MAX: usize = 300;
const REFS_MAX: usize = 32;
const ENV_KEY_MAX: usize = 128;
/// The tool call waits this long for the owner. The team bridge gives up on a
/// call after 60 seconds, so this stays under it.
const WAIT_SECS: u64 = 50;
const ONE_USE_FOLDER: &str = "one-use";

// ---------------------------------------------------------------------------
// Names

pub(crate) fn valid_name(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// Variables a bot may never point at a secret: the ones that decide how a
/// process starts or where it looks, and Retinue's own. Secret-looking names
/// (`API_KEY`, `TOKEN`) are allowed, since that is the point.
fn env_key_refused(key: &str) -> bool {
    const REFUSED: [&str; 31] = [
        "PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "PSMODULEPATH", "HOME", "HOMEDRIVE", "HOMEPATH",
        "USERPROFILE", "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)", "TEMP", "TMP", "USERNAME",
        "NODE_OPTIONS", "NODE_PATH", "PYTHONPATH", "PYTHONHOME", "LD_PRELOAD", "LD_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES",
        "CLAUDE_CONFIG_DIR", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "OPENCODE_CONFIG_CONTENT",
    ];
    let upper = key.to_ascii_uppercase();
    REFUSED.contains(&upper.as_str()) || upper.starts_with("RETINUE_") || upper.starts_with("RETINUE_")
}

fn env_key_valid(key: &str) -> bool {
    let mut bytes = key.bytes();
    key.len() <= ENV_KEY_MAX
        && bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

/// The `{ENV_KEY: secretName}` map of an `agent.create` or `agent.update` call.
/// `null` is an empty map. Each secret name must be well formed, but need not
/// exist yet: a missing one fails the turn at spawn, naming it.
pub(crate) fn parse_refs(value: &Value) -> Result<BTreeMap<String, String>, DispatchError> {
    let object = match value {
        Value::Null => return Ok(BTreeMap::new()),
        Value::Object(object) => object,
        _ => return Err(invalid("envRefs must be an object of variable names and secret names")),
    };
    if object.len() > REFS_MAX { return Err(invalid("envRefs may hold at most 32 entries")); }
    let mut refs = BTreeMap::new();
    for (key, name) in object {
        if !env_key_valid(key) || env_key_refused(key) { return Err(invalid("envRefs contains an unsupported variable name")); }
        let Some(name) = name.as_str().filter(|name| valid_name(name)) else { return Err(invalid("envRefs contains an invalid secret name")); };
        // Environment names are case-insensitive on Windows: one reference per name.
        if refs.keys().any(|existing: &String| existing.eq_ignore_ascii_case(key)) { return Err(invalid("envRefs names the same variable twice")); }
        refs.insert(key.clone(), name.to_owned());
    }
    Ok(refs)
}

/// `agent` with its `envRefs` (variable to secret name; never a value) added.
pub(crate) fn with_refs(st: &AppState, mut agent: Value) -> Value {
    if let Some(id) = agent["id"].as_str() {
        let refs = st.db.env_refs_get(id).unwrap_or_default();
        agent["envRefs"] = json!(refs);
    }
    agent
}

pub(crate) fn with_refs_all(st: &AppState, agents: Value) -> Value {
    match agents {
        Value::Array(list) => Value::Array(list.into_iter().map(|agent| with_refs(st, agent)).collect()),
        other => other,
    }
}

/// Tells the file store where the data folder is. Off Windows the values live
/// there; on Windows Credential Manager does not need it, so this is a no-op.
pub(crate) fn set_data_folder(dir: &std::path::Path) {
    #[cfg(any(not(windows), test))]
    store::files::set_data_folder(dir);
    #[cfg(all(windows, not(test)))]
    let _ = dir;
}

/// Reads a bot's referenced secrets into the spawn environment. A missing one
/// is an error naming the secret and never a value.
pub(crate) fn add_to_environment(st: &AppState, agent_id: Option<&str>, env: &mut BTreeMap<String, String>) -> Result<(), DispatchError> {
    let Some(agent_id) = agent_id else { return Ok(()) };
    let refs = st.db.env_refs_get(agent_id).unwrap_or_default();
    for (key, name) in refs {
        match store::read(&name) {
            Ok(Some(value)) => { env.insert(key, value.as_str().to_owned()); }
            Ok(None) => return Err(derr("conflict", &format!("The secret {name} is missing from the credential store, so this bot can't start. Add it again in Settings, or remove it from the bot."), StatusCode::CONFLICT)),
            Err(_) => return Err(derr("unavailable", &format!("The secret {name} could not be read because the credential store is unavailable."), StatusCode::SERVICE_UNAVAILABLE)),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The credential store

pub(crate) mod store {
    use super::*;

    /// Test builds keep secrets in memory so no test touches the real store. The
    /// one round-trip test calls `wincred` directly.
    #[cfg(test)]
    use self::in_memory as backend;
    #[cfg(test)]
    #[allow(clippy::items_after_test_module)]
    mod in_memory {
        use super::*;
        type Entries = BTreeMap<String, (Vec<u8>, String)>;
        fn memory() -> &'static StdMutex<Entries> {
            static MEMORY: OnceLock<StdMutex<Entries>> = OnceLock::new();
            MEMORY.get_or_init(Default::default)
        }
        pub fn write(name: &str, value: &[u8]) -> Result<(), String> {
            memory().lock().unwrap().insert(name.to_owned(), (value.to_vec(), Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)));
            Ok(())
        }
        pub fn read(name: &str) -> Result<Option<Zeroizing<String>>, String> {
            Ok(memory().lock().unwrap().get(name).map(|(bytes, _)| Zeroizing::new(String::from_utf8_lossy(bytes).into_owned())))
        }
        pub fn delete(name: &str) -> Result<bool, String> { Ok(memory().lock().unwrap().remove(name).is_some()) }
        pub fn list() -> Result<Vec<(String, String)>, String> { Ok(memory().lock().unwrap().iter().map(|(name, (_, at))| (name.clone(), at.clone())).collect()) }
    }
    #[cfg(all(windows, not(test)))]
    use super::wincred as backend;
    /// 17 November 2026 (multi-device piece 8, headless Linux): off Windows the
    /// values are owner-only files under `<data>/secrets`, as T3 Code's server
    /// does. The owner's rule (`AGENTS.md`, 17 November) allows this for Linux
    /// only; it is never the database, a backup, a log or a diagnostics bundle.
    ///
    /// **Not wired yet:** a Linux *desktop* with a running Secret Service
    /// keyring. There is no Linux desktop app to give it a user, so the seam is
    /// named here rather than half-built; a headless host is the only Linux
    /// shape that exists today, and it uses the files.
    #[cfg(all(not(windows), not(test)))]
    mod backend {
        use super::*;

        fn folder() -> Result<std::path::PathBuf, String> {
            super::files::folder().ok_or_else(|| "Retinue's data folder is not ready yet.".to_string())
        }
        pub fn write(name: &str, value: &[u8]) -> Result<(), String> { super::files::write_in(&folder()?, name, value) }
        pub fn read(name: &str) -> Result<Option<Zeroizing<String>>, String> { super::files::read_in(&folder()?, name) }
        pub fn delete(name: &str) -> Result<bool, String> { super::files::delete_in(&folder()?, name) }
        pub fn list() -> Result<Vec<(String, String)>, String> { super::files::list_in(&folder()?) }
    }

    pub fn write(name: &str, value: &str) -> Result<(), String> { backend::write(name, value.as_bytes()) }
    pub fn read(name: &str) -> Result<Option<Zeroizing<String>>, String> { backend::read(name) }
    pub fn delete(name: &str) -> Result<bool, String> { backend::delete(name) }
    /// `(name, updatedAt)` for every Retinue secret.
    pub fn list() -> Result<Vec<(String, String)>, String> { backend::list() }
    /// Which store the values live in, for `console.status` and the Doctor. A
    /// name, never a value.
    pub fn backend_name() -> &'static str {
        if cfg!(windows) { "windows-credential-manager" } else { "files" }
    }

    // -----------------------------------------------------------------------
    // The file store (any platform, so its tests run here and on Linux)

    // On Windows this module is compiled for its tests only: the values live in
    // Credential Manager there.
    #[allow(dead_code)]
    pub(crate) mod files {
        use super::*;
        use std::io::Write;
        use std::path::{Path, PathBuf};

        const FOLDER: &str = "secrets";
        const SUFFIX: &str = ".secret";

        /// The data folder the files live under, set once at startup. `None`
        /// before the daemon knows it, which is an error rather than a guess.
        static DATA: OnceLock<PathBuf> = OnceLock::new();

        pub(crate) fn set_data_folder(dir: &Path) {
            let _ = DATA.set(dir.to_path_buf());
        }

        pub(crate) fn folder() -> Option<PathBuf> {
            DATA.get().map(|dir| dir.join(FOLDER))
        }

        /// A name is hex-encoded, so no name can leave the folder: `vault/<id>/
        /// password` and `..` both become plain hex characters, and the mapping
        /// is reversible for `list`.
        fn file_name(name: &str) -> String {
            let mut hex = String::with_capacity(name.len() * 2 + SUFFIX.len());
            for byte in name.as_bytes() {
                hex.push_str(&format!("{byte:02x}"));
            }
            hex.push_str(SUFFIX);
            hex
        }

        fn name_of(file: &str) -> Option<String> {
            let hex = file.strip_suffix(SUFFIX)?;
            if hex.is_empty() || hex.len() % 2 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok()).collect::<Option<_>>()?;
            let name = String::from_utf8(bytes).ok()?;
            valid_name(&name).then_some(name)
        }

        /// Creates the folder, owner only. A failure is an error: a secret must
        /// never be written into a folder others can read.
        fn prepare(dir: &Path) -> Result<(), String> {
            std::fs::create_dir_all(dir).map_err(|_| "The secrets folder could not be created.".to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|_| "The secrets folder could not be made owner only.".to_string())?;
            }
            Ok(())
        }

        /// Writes through a private temp file and renames it into place, so a
        /// reader sees either the old value or the new one and never a partial
        /// file (the shape of T3 Code's `ServerSecretStore.set`).
        pub(crate) fn write_in(dir: &Path, name: &str, value: &[u8]) -> Result<(), String> {
            validate(name)?;
            prepare(dir)?;
            let target = dir.join(file_name(name));
            let temp = dir.join(format!("{}.{}.tmp", file_name(name), Uuid::new_v4()));
            let written = (|| -> std::io::Result<()> {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&temp)?;
                file.write_all(value)?;
                file.sync_all()?;
                drop(file);
                std::fs::rename(&temp, &target)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))?;
                }
                Ok(())
            })();
            if written.is_err() {
                let _ = std::fs::remove_file(&temp);
                return Err("The secret could not be saved.".to_string());
            }
            Ok(())
        }

        pub(crate) fn read_in(dir: &Path, name: &str) -> Result<Option<Zeroizing<String>>, String> {
            validate(name)?;
            let path = dir.join(file_name(name));
            match std::fs::read(&path) {
                Ok(bytes) => String::from_utf8(bytes).map(|text| Some(Zeroizing::new(text))).map_err(|_| "That secret does not hold text.".to_string()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err("The secret could not be read.".to_string()),
            }
        }

        pub(crate) fn delete_in(dir: &Path, name: &str) -> Result<bool, String> {
            validate(name)?;
            match std::fs::remove_file(dir.join(file_name(name))) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(_) => Err("The secret could not be deleted.".to_string()),
            }
        }

        /// Off Windows the backend's `list` reports only names `secret.*` would
        /// accept, exactly as Credential Manager's does (internal names such as
        /// `vault/<id>/password` are stored but never listed).
        pub(crate) fn list_in(dir: &Path) -> Result<Vec<(String, String)>, String> {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(_) => return Err("The stored secrets could not be listed.".to_string()),
            };
            let mut out = Vec::new();
            for entry in entries.flatten() {
                let Some(file) = entry.file_name().to_str().map(str::to_owned) else { continue };
                let Some(name) = name_of(&file) else { continue };
                if !valid_name(&name) { continue; }
                let at = entry
                    .metadata()
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|since| chrono::DateTime::<Utc>::from_timestamp(since.as_secs() as i64, 0))
                    .map(|at| at.to_rfc3339_opts(SecondsFormat::Millis, true))
                    .unwrap_or_default();
                out.push((name, at));
            }
            out.sort();
            Ok(out)
        }

        /// Any name this module writes: the internal callers use slashed names
        /// (`vault/<id>/password`, `connector/<id>`) that `secret.*` itself would
        /// refuse, so the store only rules out what could break a file.
        fn validate(name: &str) -> Result<(), String> {
            let ok = !name.is_empty() && name.len() <= 200 && !name.contains('\0');
            if ok { Ok(()) } else { Err("That secret name is not valid.".to_string()) }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            fn temp(name: &str) -> PathBuf {
                let dir = std::env::temp_dir().join(format!("retinue-secret-files-{name}-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&dir).unwrap();
                dir
            }

            #[test]
            fn a_value_round_trips_and_a_missing_one_is_none() {
                let dir = temp("roundtrip");
                assert!(read_in(&dir, "GITHUB_TOKEN").unwrap().is_none());
                write_in(&dir, "GITHUB_TOKEN", b"a-value").unwrap();
                assert_eq!(read_in(&dir, "GITHUB_TOKEN").unwrap().unwrap().as_str(), "a-value");
                write_in(&dir, "GITHUB_TOKEN", b"replaced").unwrap();
                assert_eq!(read_in(&dir, "GITHUB_TOKEN").unwrap().unwrap().as_str(), "replaced", "a write replaces in place");
                assert!(delete_in(&dir, "GITHUB_TOKEN").unwrap());
                assert!(!delete_in(&dir, "GITHUB_TOKEN").unwrap(), "deleting twice is not an error");
                let _ = std::fs::remove_dir_all(&dir);
            }

            #[test]
            fn names_with_slashes_and_dots_stay_inside_the_folder() {
                let dir = temp("names");
                // The internal callers use names `secret.*` would refuse; they are
                // stored, read and deleted, and never listed.
                for name in ["vault/3f2a/password", "connector/abc", "..", "a.b-c_d"] {
                    write_in(&dir, name, b"value").unwrap();
                    assert_eq!(read_in(&dir, name).unwrap().unwrap().as_str(), "value");
                }
                let listed: Vec<String> = list_in(&dir).unwrap().into_iter().map(|(name, _)| name).collect();
                assert_eq!(listed, vec!["a.b-c_d".to_string()], "only a name `secret.*` accepts is listed");
                assert!(delete_in(&dir, "vault/3f2a/password").unwrap());
                assert!(read_in(&dir, "vault/3f2a/password").unwrap().is_none());
                let files: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().filter_map(|entry| entry.file_name().to_str().map(str::to_owned)).collect();
                assert!(files.iter().all(|file| file.ends_with(SUFFIX)), "every file is a secret file: {files:?}");
                assert_eq!(files.len(), 3, "the four names, one deleted, became four plain hex names");
                assert!(files.iter().all(|file| !file.contains('/') && !file.contains('.')));
                let _ = std::fs::remove_dir_all(&dir);
            }

            #[test]
            fn a_write_leaves_no_temp_file_behind_and_refuses_an_unusable_name() {
                let dir = temp("temp");
                write_in(&dir, "one", b"value").unwrap();
                let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().filter_map(|entry| entry.file_name().to_str().map(str::to_owned)).collect();
                assert_eq!(left.len(), 1, "only the secret file remains: {left:?}");
                assert!(write_in(&dir, "", b"value").is_err());
                assert!(write_in(&dir, "with\0nul", b"value").is_err());
                assert!(write_in(&dir, &"x".repeat(201), b"value").is_err());
                assert!(read_in(&dir, "").is_err());
                let _ = std::fs::remove_dir_all(&dir);
            }

            #[cfg(unix)]
            #[test]
            fn the_folder_is_0700_and_each_file_is_0600() {
                use std::os::unix::fs::PermissionsExt;
                let dir = temp("modes");
                write_in(&dir, "one", b"value").unwrap();
                let folder = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
                assert_eq!(folder, 0o700);
                let file = std::fs::metadata(dir.join(file_name("one"))).unwrap().permissions().mode() & 0o777;
                assert_eq!(file, 0o600);
                let _ = std::fs::remove_dir_all(&dir);
            }

            #[test]
            fn listing_is_sorted_and_ignores_anything_it_did_not_write() {
                let dir = temp("list");
                write_in(&dir, "b", b"1").unwrap();
                write_in(&dir, "a", b"1").unwrap();
                std::fs::write(dir.join("stray.txt"), b"not ours").unwrap();
                let listed = list_in(&dir).unwrap();
                assert_eq!(listed.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(), vec!["a".to_string(), "b".to_string()]);
                assert!(!listed[0].1.is_empty(), "an updated time is reported");
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) mod wincred {
    use super::*;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, FILETIME};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredEnumerateW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    const PREFIX: &str = "Retinue/";

    fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() }

    fn target(name: &str) -> Vec<u16> { wide(&format!("{PREFIX}{name}")) }

    /// Fixed text only: the Win32 error code never carries a value.
    fn failure(what: &str) -> String { format!("The credential store refused to {what} (error {}).", unsafe { GetLastError() }) }

    pub fn write(name: &str, value: &[u8]) -> Result<(), String> {
        let mut target = target(name);
        let mut user = wide("Retinue");
        let mut blob = Zeroizing::new(value.to_vec());
        let credential = CREDENTIALW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            Comment: null_mut(),
            LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: null_mut(),
            TargetAlias: null_mut(),
            UserName: user.as_mut_ptr(),
        };
        // SAFETY: every pointer in `credential` points at a buffer that outlives the call.
        if unsafe { CredWriteW(&credential, 0) } == 0 { return Err(failure("save that secret")); }
        Ok(())
    }

    pub fn read(name: &str) -> Result<Option<Zeroizing<String>>, String> {
        let target = target(name);
        let mut found: *mut CREDENTIALW = null_mut();
        // SAFETY: `found` receives a buffer that `CredFree` releases below.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND { Ok(None) } else { Err(failure("read that secret")) };
        }
        // SAFETY: a successful read gives a valid credential whose blob is `CredentialBlobSize` bytes.
        let bytes = unsafe {
            let credential = &*found;
            let slice = if credential.CredentialBlob.is_null() || credential.CredentialBlobSize == 0 { &[][..] } else { std::slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize) };
            let copy = Zeroizing::new(slice.to_vec());
            // Wipe the system's copy before releasing it.
            if !credential.CredentialBlob.is_null() { std::ptr::write_bytes(credential.CredentialBlob, 0, credential.CredentialBlobSize as usize); }
            CredFree(found as *const _);
            copy
        };
        String::from_utf8(bytes.to_vec()).map(|text| Some(Zeroizing::new(text))).map_err(|_| "That secret does not hold text.".to_owned())
    }

    pub fn delete(name: &str) -> Result<bool, String> {
        let target = target(name);
        // SAFETY: `target` is a null-terminated wide string.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND { Ok(false) } else { Err(failure("delete that secret")) };
        }
        Ok(true)
    }

    fn filetime_to_rfc3339(time: &FILETIME) -> String {
        let ticks = ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
        let seconds = (ticks / 10_000_000) as i64 - 11_644_473_600;
        chrono::DateTime::<Utc>::from_timestamp(seconds, 0).map(|at| at.to_rfc3339_opts(SecondsFormat::Millis, true)).unwrap_or_default()
    }

    pub fn list() -> Result<Vec<(String, String)>, String> {
        let filter = wide(&format!("{PREFIX}*"));
        let (mut count, mut items): (u32, *mut *mut CREDENTIALW) = (0, null_mut());
        // SAFETY: on success `items` points at `count` credential pointers, released with `CredFree`.
        if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut items) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND { Ok(Vec::new()) } else { Err(failure("list secrets")) };
        }
        let mut out = Vec::new();
        for index in 0..count as usize {
            // SAFETY: `index` is below `count`, and each entry is a valid credential.
            let credential = unsafe { &**items.add(index) };
            if credential.Type != CRED_TYPE_GENERIC || credential.TargetName.is_null() { continue; }
            let mut length = 0;
            // SAFETY: the target name is a null-terminated wide string.
            while unsafe { *credential.TargetName.add(length) } != 0 { length += 1; }
            let full = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(credential.TargetName, length) });
            if let Some(name) = full.strip_prefix(PREFIX).filter(|name| valid_name(name)) { out.push((name.to_owned(), filetime_to_rfc3339(&credential.LastWritten))); }
        }
        // SAFETY: `items` came from `CredEnumerateW`.
        unsafe { CredFree(items as *const _) };
        out.sort();
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// RPC

fn confirmed(p: &Value) -> Result<(), DispatchError> {
    if p["confirm"] == true { Ok(()) } else { Err(invalid("confirm: true is required")) }
}

fn name_of(p: &Value) -> Result<&str, DispatchError> {
    p["name"].as_str().filter(|name| valid_name(name)).ok_or_else(|| invalid("name must be 1 to 64 letters, digits, dots, dashes or underscores"))
}

fn unavailable(message: String) -> DispatchError {
    derr("unavailable", &message, StatusCode::SERVICE_UNAVAILABLE)
}

fn storage_error(_: retinue_storage::StorageError) -> DispatchError {
    derr("internal", "secret storage operation failed", StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "secret.list" => {
            let listed = store::list().map_err(unavailable)?;
            let mut secrets = Vec::with_capacity(listed.len());
            for (name, updated_at) in listed {
                let used_by = st.db.env_ref_users(&name).map_err(storage_error)?;
                secrets.push(json!({"name": name, "updatedAt": updated_at, "usedBy": used_by}));
            }
            Ok(json!({"secrets": secrets}))
        }
        "secret.set" => {
            confirmed(p)?;
            let name = name_of(p)?;
            // The value is checked and written without being copied anywhere else.
            let value = p["value"].as_str().filter(|value| !value.is_empty() && value.len() <= VALUE_MAX && !value.contains('\0'))
                .ok_or_else(|| invalid("value must be 1 to 2,048 bytes of text"))?;
            store::write(name, value).map_err(unavailable)?;
            Ok(json!({"name": name}))
        }
        "secret.delete" => {
            confirmed(p)?;
            let name = name_of(p)?;
            let users = st.db.env_ref_users(name).map_err(storage_error)?;
            if !users.is_empty() {
                return Err(derr("conflict", &format!("The secret {name} is used by {}. Remove it from them first.", users.join(", ")), StatusCode::CONFLICT));
            }
            store::delete(name).map_err(unavailable)?;
            Ok(json!({"deleted": true}))
        }
        "secret.request.list" => Ok(json!({"requests": pending_rows()})),
        "secret.request.answer" => {
            let id = p["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64).ok_or_else(|| invalid("id is required"))?;
            let value = p["value"].as_str().filter(|value| !value.is_empty() && value.len() <= ONE_USE_MAX && !value.contains('\0'))
                .ok_or_else(|| invalid("value must be 1 to 16,384 bytes of text"))?;
            let sender = take_waiter(id).ok_or_else(|| derr("conflict", "That request is no longer waiting.", StatusCode::CONFLICT))?;
            // A tool call that gave up a moment ago has dropped its receiver: the value goes nowhere.
            sender.send(Answer::Value(Zeroizing::new(value.to_owned()))).map_err(|_| derr("conflict", "That request is no longer waiting.", StatusCode::CONFLICT))?;
            Ok(json!({"answered": true}))
        }
        "secret.request.decline" => {
            let id = p["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64).ok_or_else(|| invalid("id is required"))?;
            let sender = take_waiter(id).ok_or_else(|| derr("conflict", "That request is no longer waiting.", StatusCode::CONFLICT))?;
            let _ = sender.send(Answer::Declined);
            Ok(json!({"declined": true}))
        }
        _ => Err(derr("not_found", "unknown secret method", StatusCode::NOT_FOUND)),
    }
}

// ---------------------------------------------------------------------------
// One-use requests

enum Answer {
    Value(Zeroizing<String>),
    Declined,
}

struct Pending {
    session_id: String,
    name: String,
    reason: String,
    bot_name: String,
    expires_at: String,
    /// Taken by the answer or the decline; the waiting tool call owns the other end.
    waiter: Option<oneshot::Sender<Answer>>,
}

/// Pending requests live in memory only: a request never reaches the database,
/// and a restart ends them all. Ids are unique, so tests that share the process
/// do not see each other's.
fn requests() -> &'static StdMutex<HashMap<String, Pending>> {
    static REQUESTS: OnceLock<StdMutex<HashMap<String, Pending>>> = OnceLock::new();
    REQUESTS.get_or_init(Default::default)
}

fn take_waiter(id: &str) -> Option<oneshot::Sender<Answer>> {
    requests().lock().unwrap().get_mut(id).and_then(|pending| pending.waiter.take())
}

fn row_of(id: &str, pending: &Pending) -> Value {
    json!({"id": id, "sessionId": pending.session_id, "name": pending.name, "reason": pending.reason, "botName": pending.bot_name, "expiresAt": pending.expires_at})
}

fn pending_rows() -> Vec<Value> {
    let mut rows: Vec<(String, Value)> = requests().lock().unwrap().iter().filter(|(_, pending)| pending.waiter.is_some()).map(|(id, pending)| (pending.expires_at.clone(), row_of(id, pending))).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

const SENTENCE_UNATTENDED: &str = "request_secret is not available here: nobody is watching this conversation to answer. Ask for it in a conversation the owner is in.";

/// The MCP tool description, offered wherever someone can answer.
pub fn tool_schema() -> Value {
    json!({
        "name": "request_secret",
        "description": "Ask the owner for one secret (a token or key) for this run only. The owner sees a private form and may decline. If they give it, you get the path of a private file that holds it: read it when you need it and do not print it. The file is deleted when your turn ends. The call waits for the owner for up to 50 seconds.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "What the secret is called, 1 to 64 letters, digits, dots, dashes or underscores (for example GITHUB_TOKEN)."},
                "reason": {"type": "string", "description": "Why you need it, up to 300 characters. The owner reads this."}
            },
            "required": ["name", "reason"],
            "additionalProperties": false
        }
    })
}

fn one_use_dir(st: &AppState) -> Option<PathBuf> {
    st.team.data_dir.lock().ok().and_then(|dir| dir.clone()).map(|dir| dir.join(ONE_USE_FOLDER))
}

/// Deletes every file left in the one-use folder. Called at startup, before any turn can run.
pub(crate) fn sweep_leftovers(data_dir: &std::path::Path) {
    let dir = data_dir.join(ONE_USE_FOLDER);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() { let _ = std::fs::remove_file(path); }
    }
}

fn status_event(id: &str, pending: &Pending, status: &str) -> Value {
    let mut payload = row_of(id, pending);
    payload["status"] = json!(status);
    payload
}

pub async fn request_secret_tool(st: &AppState, session_id: &str, args: &Value) -> Result<String, String> {
    request_secret_with(st, session_id, args, Duration::from_secs(WAIT_SECS)).await
}

pub(crate) async fn request_secret_with(st: &AppState, session_id: &str, args: &Value, wait: Duration) -> Result<String, String> {
    let object = args.as_object().ok_or("request_secret input must be an object.")?;
    if let Some(key) = object.keys().find(|key| !matches!(key.as_str(), "name" | "reason")) { return Err(format!("request_secret input has an unsupported field: {key}.")); }
    let name = args["name"].as_str().map(str::trim).filter(|name| valid_name(name)).ok_or("name must be 1 to 64 letters, digits, dots, dashes or underscores.")?.to_owned();
    let reason = args["reason"].as_str().map(str::trim).filter(|reason| !reason.is_empty()).ok_or("reason is required.")?;
    if reason.chars().count() > REASON_MAX { return Err(format!("reason must be at most {REASON_MAX} characters.")); }
    let reason = reason.to_owned();
    let session = st.db.session_detail(session_id).map_err(|_| "This conversation is no longer available.".to_owned())?;
    if crate::questions::may_ask(st, &session).is_err() { return Err(SENTENCE_UNATTENDED.to_owned()); }
    let turn_id = st.active_turns.lock().await.get(session_id).cloned().ok_or("This conversation has no running turn to give a secret to.")?;
    let bot_name = session["agentId"].as_str().and_then(|id| st.db.agent_get(id).ok()).and_then(|agent| agent["name"].as_str().map(str::to_owned)).unwrap_or_else(|| "your bot".to_owned());
    let dir = one_use_dir(st).ok_or("The folder for one-use secrets is unavailable.")?;

    let id = Uuid::new_v4().to_string();
    let (sender, mut receiver) = oneshot::channel();
    let expires_at = (Utc::now() + ChronoDuration::milliseconds(wait.as_millis() as i64)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let announced = {
        let mut map = requests().lock().unwrap();
        // One open request per conversation: a second would only stack forms.
        if map.values().any(|pending| pending.session_id == session_id && pending.waiter.is_some()) { return Err("You already have a request open. Wait for the owner's answer to it.".to_owned()); }
        let pending = Pending { session_id: session_id.to_owned(), name: name.clone(), reason, bot_name: bot_name.clone(), expires_at, waiter: Some(sender) };
        let event = status_event(&id, &pending, "pending");
        map.insert(id.clone(), pending);
        event
    };
    st.emit("secret.request.changed", announced).await;

    // The turn waits here. It ends on an answer, a decline, the end of the turn
    // (a cancel included) or the time limit.
    let deadline = tokio::time::Instant::now() + wait;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let outcome = loop {
        tokio::select! {
            answer = &mut receiver => break answer.map(Some).unwrap_or(None),
            _ = tick.tick() => {
                if st.active_turns.lock().await.get(session_id) != Some(&turn_id) { break None; }
                if tokio::time::Instant::now() >= deadline { break None; }
            }
        }
    };
    // Close the request, then say how it ended. Taking the waiter first means a
    // late answer finds nothing to send to and is refused.
    let pending = requests().lock().unwrap().remove(&id);
    let Some(pending) = pending else { return Err("The request was lost.".to_owned()) };
    // An answer can land between the last check and the close: it is still in the channel.
    let outcome = outcome.or_else(|| receiver.try_recv().ok());
    match outcome {
        Some(Answer::Value(value)) => {
            let path = match write_one_use_file(&dir, &id, &value) {
                Ok(path) => path,
                Err(error) => { st.emit("secret.request.changed", status_event(&id, &pending, "cancelled")).await; return Err(error); }
            };
            watch_turn_end(st.clone(), session_id.to_owned(), turn_id, path.clone());
            st.emit("secret.request.changed", status_event(&id, &pending, "answered")).await;
            team::note(st, session_id, &format!("You gave {bot_name} {name} for this turn."), json!({"kind": "secret_given"})).await;
            Ok(format!("The owner gave you {name} in this file: {}. Read it when you need it, and don't print it. It is deleted when this turn ends.", path.display()))
        }
        Some(Answer::Declined) => {
            st.emit("secret.request.changed", status_event(&id, &pending, "declined")).await;
            Ok(format!("The owner declined to give you {name}. Carry on without it, or say what is blocked."))
        }
        None => {
            let ended = st.active_turns.lock().await.get(session_id) != Some(&turn_id);
            st.emit("secret.request.changed", status_event(&id, &pending, if ended { "cancelled" } else { "expired" })).await;
            if ended { Err("The turn ended before the owner answered.".to_owned()) } else { Ok(format!("The owner did not answer in time, so {name} was not given. Ask again only if you still need it.")) }
        }
    }
}

/// Creates the file empty, restricts it to the current user, and only then writes
/// the value, so the value is never in a file others can read.
fn write_one_use_file(dir: &std::path::Path, id: &str, value: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|_| "The folder for one-use secrets could not be created.".to_owned())?;
    let path = dir.join(id);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_| "The private file could not be created.".to_owned())?;
    crate::remote::native::apply_key_permissions(&path);
    if file.write_all(value.as_bytes()).and_then(|_| file.flush()).is_err() {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err("The private file could not be written.".to_owned());
    }
    Ok(path)
}

/// Deletes the file once the turn that was given it is over.
fn watch_turn_end(st: AppState, session_id: String, turn_id: String, path: PathBuf) {
    tokio::spawn(async move {
        loop {
            if st.active_turns.lock().await.get(&session_id) != Some(&turn_id) { break; }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let _ = std::fs::remove_file(&path);
    });
}
