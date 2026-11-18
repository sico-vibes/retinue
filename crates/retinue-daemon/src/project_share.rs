//! Projects on another machine, from the owner's own devices (17 November 2026).
//!
//! The owner's amendment of that date lets a paired device list folders under one base folder the
//! host's owner sets, register a folder there as a project, and clone into it. Everything here is
//! confined to that folder: a path is refused when it is not absolute, holds a `..` segment, or does
//! not resolve inside the base folder, and a symlink or junction that resolves outside is not
//! listed at all. Nothing here reads a file's contents and nothing lists a size.
//!
//! The window at the far machine never calls these; its daemon does, over the link, and the call
//! arrives through this machine's own gateway, so the tier and the one activity line are enforced
//! where a phone's call would be. The switch is off until the host's owner turns it on
//! (`projects.devices.enabled`), and the base folder is a setting of this machine
//! (`projects.devices.folder`) that never travels in a backup.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

/// Whether a paired device may use this surface at all. Off unless the owner turned it on here.
pub const SETTING_ENABLED: &str = "projects.devices.enabled";
/// The one folder a paired device may browse and write in.
pub const SETTING_FOLDER: &str = "projects.devices.folder";
/// The most entries one listing returns; `truncated` says one was dropped.
const MAX_ENTRIES: usize = 500;
/// The sentence every refusal outside the base folder gives, so a device learns nothing about the
/// host's disk beyond "no".
const OUTSIDE: &str = "That folder is outside the folder this computer shares with other machines.";

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn denied(message: &str) -> DispatchError {
    derr("forbidden_remote", message, StatusCode::FORBIDDEN)
}

/// Whether the gateway marked this call as a device's. The gateway stamps it, and a device cannot
/// clear it, so `project.clone` can confine a device without confining the desktop.
pub fn is_device_call(p: &Value) -> bool {
    p.get("deviceCall").and_then(Value::as_bool).unwrap_or(false)
}

/// The base folder as the disk knows it, or the sentence a device gets. This is the switch, the
/// setting and the folder's own existence in one place, so every method refuses identically.
pub fn base_folder(st: &AppState) -> Result<PathBuf, DispatchError> {
    let settings = st.db.settings().unwrap_or(Value::Null);
    if settings.get(SETTING_ENABLED) != Some(&Value::Bool(true)) {
        return Err(denied("Projects from other machines are off on this computer. Turn them on in Settings › Machines."));
    }
    let configured = settings.get(SETTING_FOLDER).and_then(Value::as_str).map(str::trim).unwrap_or("");
    if configured.is_empty() {
        return Err(denied("Choose the folder other machines may use for new projects in Settings › Machines first."));
    }
    let missing = || denied("The folder other machines may use for new projects is missing on this computer.");
    let base = std::fs::canonicalize(configured).map_err(|_| missing())?;
    if !base.is_dir() {
        return Err(missing());
    }
    Ok(base)
}

/// The folder a `project.clone` from a device may create its copy in, in the form the caller gave it.
pub fn confine_parent(st: &AppState, parent: &Path) -> Result<PathBuf, DispatchError> {
    let base = base_folder(st)?;
    confined(&base, &parent.to_string_lossy())
}

/// The form of a path a person (and git) recognises: `canonicalize` answers `\\?\C:\…`, which git
/// refuses as a clone target, so the reply to a window never carries that prefix. `pub(crate)` so the
/// tests compare a reply with this rule rather than a copy of it.
pub(crate) fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix(r"\\?\") else { return text.into_owned() };
    match rest.strip_prefix(r"UNC\") {
        Some(network) => format!(r"\\{network}"),
        None => rest.to_string(),
    }
}

/// The path as the caller gave it, once it is known to resolve inside `base`. The disk is asked with
/// `canonicalize`, so `..` and a link are judged by where they really land; the caller's own text is
/// what comes back, because git and the project row use it.
fn confined(base: &Path, raw: &str) -> Result<PathBuf, DispatchError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(invalid("Choose a folder."));
    }
    let path = Path::new(text);
    // A relative path or a `..` segment is refused before the disk is asked: a device never needs
    // either to walk a tree it was shown, and saying no here keeps the rule readable.
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err(denied(OUTSIDE));
    }
    let resolved = std::fs::canonicalize(path).map_err(|_| denied("That folder is not on this computer."))?;
    if !inside(base, &resolved) {
        return Err(denied(OUTSIDE));
    }
    Ok(path.to_path_buf())
}

/// Whether a canonical `path` is `base` itself or sits under it. Both sides are folded and compared
/// with a separator, so `C:\work two` never matches `C:\work`, and Windows' own case-insensitive
/// names cannot be used to slip past the check.
fn inside(base: &Path, path: &Path) -> bool {
    let (base, path) = (fold(base), fold(path));
    path == base || path.strip_prefix(base.as_str()).is_some_and(|rest| rest.starts_with(SEP))
}

const SEP: char = '\\';

/// A path as one comparable string: forward slashes made backslashes, the `\\?\` prefix that
/// `canonicalize` adds taken off, a trailing separator removed, and lower-cased where the disk
/// ignores case.
fn fold(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let trimmed = if text.len() > 3 { text.trim_end_matches('\\') } else { text };
    if cfg!(windows) {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_string()
    }
}

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "project.browse" => browse(st, p),
        "project.createFromFolder" => create_from_folder(st, p).await,
        _ => Err(derr("not_found", "unknown project method", StatusCode::NOT_FOUND)),
    }
}

/// `project.browse`: the immediate children of one folder inside the base folder, by name and kind.
fn browse(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let base = base_folder(st)?;
    let path = match p.get("path").and_then(Value::as_str).map(str::trim) {
        None | Some("") => base.clone(),
        Some(raw) => confined(&base, raw)?,
    };
    let resolved = std::fs::canonicalize(&path).ok().filter(|resolved| inside(&base, resolved)).ok_or_else(|| denied("That folder is not on this computer."))?;
    if !resolved.is_dir() {
        return Err(denied("That folder is not on this computer."));
    }
    let read = std::fs::read_dir(&resolved).map_err(|_| denied("That folder cannot be read on this computer."))?;
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    let mut truncated = false;
    for entry in read.flatten() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        // A link or junction is offered only when it lands inside the base folder, and then as
        // whatever it resolves to; one that leaves the folder is not named at all.
        let Ok(target) = std::fs::canonicalize(entry.path()) else { continue };
        if !inside(&base, &target) {
            continue;
        }
        let kind = if target.is_dir() { "folder" } else { "file" };
        entries.push((name, kind));
    }
    entries.sort_by_key(|(name, _)| name.to_ascii_lowercase());
    let entries: Vec<Value> = entries.into_iter().map(|(name, kind)| json!({"name": name, "kind": kind})).collect();
    let parent = resolved.parent().filter(|parent| inside(&base, parent)).map(display);
    Ok(json!({
        "base": display(&base),
        "path": display(&resolved),
        "parent": parent.map(Value::String).unwrap_or(Value::Null),
        "entries": entries,
        "truncated": truncated,
    }))
}

/// `project.createFromFolder`: register a folder inside the base folder as a project, through the
/// same `project.create` the desktop uses, so the row, its repository identity and its event match.
async fn create_from_folder(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let base = base_folder(st)?;
    let path = confined(&base, p["path"].as_str().unwrap_or(""))?;
    if !path.is_dir() {
        return Err(denied("That folder is not on this computer."));
    }
    let name = match p["name"].as_str().map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_owned(),
        None => path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
    };
    let params = json!({"name": name, "path": path.to_string_lossy()});
    let reply = crate::team::team_rpc(st, "project.create", &params).await?;
    // Only these three fields: a project row carries `overrides` (a setup script) and the shared
    // settings' state, neither of which may cross to a device.
    Ok(json!({"project": {
        "id": reply["project"]["id"].clone(),
        "name": reply["project"]["name"].clone(),
        "path": reply["project"]["path"].clone(),
    }}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_inside_the_base_is_the_only_one_accepted() {
        let base = std::env::temp_dir().join(format!("retinue-ws-{}", uuid::Uuid::new_v4()));
        let inner = base.join("work").join("repo");
        std::fs::create_dir_all(&inner).unwrap();
        let sibling = base.with_file_name(format!("{}-sibling", base.file_name().unwrap().to_string_lossy()));
        std::fs::create_dir_all(&sibling).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        let inner = std::fs::canonicalize(&inner).unwrap();
        let sibling = std::fs::canonicalize(&sibling).unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        // Inside, at any depth, is accepted and comes back canonical.
        assert_eq!(confined(&base, &inner.to_string_lossy()).unwrap(), inner);
        // `..`, an absolute path beside the base, and a relative path are all refused with the same
        // sentence.
        for raw in [
            format!("{}{sep}..", base.to_string_lossy()),
            format!("{}{sep}..{sep}{}", inner.to_string_lossy(), sibling.file_name().unwrap().to_string_lossy()),
            sibling.to_string_lossy().into_owned(),
            "work".to_string(),
            "".to_string(),
        ] {
            let refused = confined(&base, &raw).unwrap_err();
            assert!(
                refused.1.message == OUTSIDE || refused.1.message == "Choose a folder.",
                "{raw}: {}",
                refused.1.message
            );
        }
        // A prefix is not a child: `C:\work two` must not pass as inside `C:\work`.
        assert!(!inside(&base, &sibling));
        let inside_one = base.join("work");
        std::fs::create_dir_all(&inside_one).unwrap();
        assert!(inside(&base, &std::fs::canonicalize(&inside_one).unwrap()));
        assert!(inside(&base, &base));
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&sibling);
    }
}
