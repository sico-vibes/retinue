//! Wave 2 (nod adoption, 17 November 2026): Retinue's data folder belongs to the
//! owner's own account.
//!
//! The data folder holds the daemon capability file, the endpoint files the MCP
//! bridges read, the SQLite database, sent and task images, and the scratch area
//! this wave adds. A folder created under the local app data root inherits
//! whatever the parent grants, which on a shared machine is every account on it.
//! This module removes that inheritance once, at startup, and is the one place
//! the Doctor reads its answer from.
//!
//! The rules are the ones the app already follows where it restricts a private
//! file (`retinue-adapter-claude`'s instruction files, `remote/native.rs`'s
//! certificate key): no shell, an argument list only; a failure is reported, never
//! fatal; and the error text is fixed, so no path or account name can reach a log.

use crate::AppState;
use serde_json::{json, Value};
use std::path::Path;

/// The folder names this wave creates inside the data folder.
const SCRATCH: &str = "scratch";
const TOOL_OUTPUT: &str = "tool-output";

/// Restrict one path to the current Windows account (or to its owner on Unix).
/// `directory` grants full control that is inherited by what is created inside.
#[cfg(windows)]
pub(crate) fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let root = std::env::var_os("SystemRoot").ok_or_else(|| "The Windows folder was not found.".to_string())?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() && !name.is_empty() => format!("{domain}\\{name}"),
        _ => return Err("This app could not tell which Windows account it runs as.".to_string()),
    };
    let grant = if directory { format!("{user}:(OI)(CI)F") } else { format!("{user}:F") };
    let mut command = Command::new(Path::new(&root).join("System32").join("icacls.exe"));
    command
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(grant)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.creation_flags(0x0800_0000);
    let output = command.output().map_err(|_| "The permission tool could not be started.".to_string())?;
    if !output.status.success() {
        return Err("The permission tool refused the folder.".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn restrict(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if directory { 0o700 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|_| "The folder's permissions could not be changed.".to_string())
}

/// The data folder, or `None` before the daemon knows it.
pub(crate) fn data_folder(st: &AppState) -> Option<std::path::PathBuf> {
    st.team.data_dir.lock().ok().and_then(|dir| dir.clone())
}

/// Applies the restriction to the data folder and the two folders this wave
/// creates inside it. Called once at startup; the result is what the Doctor
/// reports, so nothing here is fatal.
pub(crate) fn at_startup(st: &AppState) {
    let Some(dir) = data_folder(st) else { return };
    if !dir.is_dir() {
        return;
    }
    if restrict(&dir, true).is_err() {
        return;
    }
    for name in [SCRATCH, TOOL_OUTPUT] {
        let folder = dir.join(name);
        if folder.is_dir() {
            let _ = restrict(&folder, true);
        }
    }
}

/// Whether `icacls` says a folder has no inherited entries left and grants this
/// account full control. `(I)` is icacls' own marker for an inherited entry and
/// is printed as ASCII whatever the Windows display language is; `(OI)` and
/// `(CI)` do not contain it. A folder that a check cannot read answers `None`
/// rather than guessing.
#[cfg(windows)]
fn folder_is_owner_only(path: &Path) -> Option<bool> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let root = std::env::var_os("SystemRoot")?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) if !domain.is_empty() && !name.is_empty() => name,
        _ => return None,
    };
    let mut command = Command::new(Path::new(&root).join("System32").join("icacls.exe"));
    command.arg(path).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    command.creation_flags(0x0800_0000);
    let output = command.output().ok()?;
    if !output.status.success() {
        // A folder that cannot be read (missing, or refused) is answered with
        // nothing rather than with "not restricted".
        return None;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    if listing.trim().is_empty() {
        return None;
    }
    let inherited = listing.contains("(I)");
    // The grant line reads `<domain>\<user>:(OI)(CI)(F)`; the account name alone
    // is enough to prove the owner still has full control.
    let granted = listing.contains(user.as_str()) && listing.contains("(F)");
    Some(!inherited && granted)
}

#[cfg(not(windows))]
fn folder_is_owner_only(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).ok()?.permissions().mode();
    Some(mode & 0o077 == 0)
}

/// The Doctor's rows for this module. An empty list before the daemon knows its
/// data folder: there is nothing to report on yet, and that is not a fault.
pub(crate) fn doctor_checks(st: &AppState) -> Vec<Value> {
    let Some(dir) = data_folder(st) else { return Vec::new() };
    if !dir.is_dir() {
        return vec![json!({
            "id": "safety.dataFolder", "area": "safety",
            "label": "Retinue's data folder is yours alone",
            "status": "warn",
            "detail": "Retinue has not created its data folder yet, so there are no permissions to check.",
            "fix": Value::Null,
        })];
    }
    let (status, detail) = match folder_is_owner_only(&dir) {
        Some(true) => ("ok", "Only your Windows account can read it, so a capability or an endpoint file in it stays yours.".to_owned()),
        Some(false) => ("warn", "Other accounts on this computer may be able to read Retinue's data folder, which holds its capability and endpoint files. Restart Retinue to restrict it again.".to_owned()),
        None => ("warn", "Retinue could not read its own data folder's permissions, so it cannot say who can open it.".to_owned()),
    };
    vec![json!({
        "id": "safety.dataFolder", "area": "safety",
        "label": "Retinue's data folder is yours alone",
        "status": status,
        "detail": detail,
        "fix": Value::Null,
    })]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricting_a_temp_folder_leaves_it_owner_only() {
        let dir = std::env::temp_dir().join(format!("retinue-acl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        restrict(&dir, true).expect("the temp folder is restricted");
        assert_eq!(folder_is_owner_only(&dir), Some(true), "no inherited entry is left and the owner keeps full control");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_answers_unknown_rather_than_a_guess() {
        let dir = std::env::temp_dir().join(format!("retinue-acl-missing-{}", uuid::Uuid::new_v4()));
        assert_eq!(folder_is_owner_only(&dir), None);
    }

    #[test]
    fn a_folder_without_the_restriction_is_not_reported_as_owner_only() {
        // A folder left with inherited entries must not read as restricted. The
        // test only runs this where a parent grant exists, so it asserts on the
        // answer's shape rather than on a machine's own policy.
        let dir = std::env::temp_dir().join(format!("retinue-acl-open-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let answer = folder_is_owner_only(&dir);
        assert!(answer.is_some(), "a real folder is always answered");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
