//! Wave 2 (nod adoption, 17 November 2026): a scratch folder per conversation.
//!
//! Bots write disposable files wherever they like in a project folder, and every
//! one of them shows up as untracked noise in a diff and in the Changes screen.
//! nod gives each thread `/workspace/<thread>` for that work; this is the cheap
//! part of the same idea without a container: `<data>/scratch/<conversation>/`,
//! named in the bot's instructions, restricted to the owner, and deleted when it
//! has not been touched for a month.
//!
//! **This is not isolation.** The folder is offered so tidiness is the easy
//! choice, not so a bot is contained: a bot still runs as the owner and can still
//! write anywhere the owner can. Nothing in the app presents it otherwise.

use crate::AppState;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// The folder name inside the data folder.
const FOLDER: &str = "scratch";
/// How long an untouched scratch folder is kept before startup removes it.
const KEEP: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// The most one conversation's folder name may be; ids are UUIDs.
const NAME_MAX: usize = 80;

/// A conversation id that is safe as one folder name, or `None`. The ids the
/// daemon makes are UUIDs; anything else is refused rather than sanitised into
/// something that could collide with another folder.
fn folder_name(session_id: &str) -> Option<&str> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() || trimmed.len() > NAME_MAX { return None; }
    let safe = trimmed.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    safe.then_some(trimmed)
}

fn base(st: &AppState) -> Option<PathBuf> {
    crate::sandbox::data_folder(st).map(|dir| dir.join(FOLDER))
}

/// The conversation's scratch folder, made when it does not exist yet. `None`
/// when the daemon has no data folder or the folder cannot be made: a missing
/// scratch folder must never stop a turn.
pub(crate) fn ensure(st: &AppState, session_id: &str) -> Option<PathBuf> {
    let name = folder_name(session_id)?;
    let folder = base(st)?.join(name);
    if !folder.is_dir() {
        std::fs::create_dir_all(&folder).ok()?;
        let _ = crate::sandbox::restrict(&folder, true);
    }
    Some(folder)
}

/// One instruction line for a conversation, or `None` when there is no folder to
/// name. Sentence case, no markup: it sits among the other instruction lines.
pub(crate) fn instruction_line(st: &AppState, session_id: &str) -> Option<String> {
    let folder = ensure(st, session_id)?;
    Some(format!(
        "Disposable files go in {folder}. That folder is outside the project, so work you leave there never reaches a diff: use it for probes, downloads and anything you would otherwise leave lying in the project.",
        folder = folder.display()
    ))
}

/// Whether a folder touched at `modified` is old enough for the startup pass to
/// remove. A clock that moved backwards reads as recent, never as stale.
fn is_stale(modified: SystemTime, now: SystemTime) -> bool {
    now.duration_since(modified).is_ok_and(|age| age >= KEEP)
}

/// Startup: remove the folders nobody has touched in a month. Bounded and quiet:
/// a folder that cannot be removed is left for the next start.
pub(crate) fn prune(st: &AppState) {
    let Some(root) = base(st) else { return };
    let Ok(entries) = std::fs::read_dir(&root) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let stale = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| is_stale(modified, now));
        if stale {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_plain_conversation_id_names_a_folder() {
        assert_eq!(folder_name("ses-1234"), Some("ses-1234"));
        assert_eq!(folder_name("  ses-1234  "), Some("ses-1234"));
        assert_eq!(folder_name(""), None);
        assert_eq!(folder_name("../escape"), None);
        assert_eq!(folder_name("a\\b"), None);
        assert_eq!(folder_name("a/b"), None);
        assert_eq!(folder_name(&"a".repeat(NAME_MAX + 1)), None);
    }

    #[test]
    fn a_folder_is_stale_after_a_month_of_no_use() {
        let now = SystemTime::now();
        assert!(!is_stale(now, now));
        assert!(!is_stale(now - Duration::from_secs(29 * 24 * 60 * 60), now));
        assert!(is_stale(now - KEEP - Duration::from_secs(60), now));
        // A clock that moved backwards reads as recent, never as stale.
        assert!(!is_stale(now + Duration::from_secs(60), now));
    }
}
