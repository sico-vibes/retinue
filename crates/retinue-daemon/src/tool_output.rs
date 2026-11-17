//! Wave 2 (nod adoption, 17 November 2026): a long tool output keeps its head and
//! its tail, and the whole text is kept on disk.
//!
//! A tail-only cut hides the error that came first, which is usually the one that
//! says what went wrong; a head-only cut hides the exit summary. nod keeps both
//! and names the complete log (`01-sandbox.md`, idea 5). This module does the same
//! for the tools Retinue itself answers: the head, a marker line saying how much
//! was left out, the tail, and the full text saved under `<data>/tool-output/`
//! where the owner (and the bot) can open it with an ordinary file read.
//!
//! Rules: the cut never splits a character, the marker is fixed text, the saved
//! folder is restricted to the owner, and a file that cannot be written is `None`
//! rather than a failed tool call.

use crate::AppState;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// The head and the tail each keep this much of a long output by the module's own
/// rule; a caller whose answer must survive another ceiling passes its own halves.
pub(crate) const HEAD_BYTES: usize = 32 * 1024;
pub(crate) const TAIL_BYTES: usize = 32 * 1024;
/// How long a saved output is kept before startup removes it.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The folder inside the data folder.
const FOLDER: &str = "tool-output";
/// The most one tool name may be, as a file name.
const TOOL_MAX: usize = 40;

/// A cut output: what the model sees, whether anything was left out, and the
/// whole text's size in bytes.
pub(crate) struct Capped {
    pub text: String,
    pub truncated: bool,
    pub bytes: usize,
}

/// The longest prefix of `text` that fits in `max` bytes without splitting a
/// character.
fn prefix(text: &str, max: usize) -> &str {
    if text.len() <= max { return text; }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) { end -= 1; }
    &text[..end]
}

/// The longest suffix of `text` that fits in `max` bytes without splitting a
/// character.
fn suffix(text: &str, max: usize) -> &str {
    if text.len() <= max { return text; }
    let mut start = text.len() - max;
    while start < text.len() && !text.is_char_boundary(start) { start += 1; }
    &text[start..]
}

/// The head, a fixed marker line, then the tail. Short text is returned whole.
pub(crate) fn head_and_tail(text: &str, head: usize, tail: usize) -> Capped {
    if text.len() <= head + tail {
        return Capped { text: text.to_owned(), truncated: false, bytes: text.len() };
    }
    let omitted = text.len() - head - tail;
    Capped {
        text: format!(
            "{head}\n… {omitted} bytes left out here; the whole output is kept, and its file is named beside it …\n{tail}",
            head = prefix(text, head),
            tail = suffix(text, tail),
        ),
        truncated: true,
        bytes: text.len(),
    }
}

/// The folder for one conversation's saved outputs, made when missing.
fn folder(st: &AppState, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.len() > 80 || !session_id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
        return None;
    }
    let root = crate::sandbox::data_folder(st)?.join(FOLDER).join(session_id);
    if !root.is_dir() {
        std::fs::create_dir_all(&root).ok()?;
        let _ = crate::sandbox::restrict(&root, true);
    }
    Some(root)
}

/// Saves one full output and answers its path. `None` when it cannot be saved:
/// the capped text is still returned, so the call never fails over this.
pub(crate) fn save(st: &AppState, session_id: &str, tool: &str, text: &str) -> Option<PathBuf> {
    let tool: String = tool.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-').take(TOOL_MAX).collect();
    if tool.is_empty() { return None; }
    let path = folder(st, session_id)?.join(format!("{tool}-{}.log", uuid::Uuid::new_v4()));
    std::fs::write(&path, text).ok()?;
    let _ = crate::sandbox::restrict(&path, false);
    Some(path)
}

/// Whether a saved file is old enough for the startup pass to remove. A clock
/// that moved backwards reads as recent, never as stale.
fn is_stale(modified: SystemTime, now: SystemTime) -> bool {
    now.duration_since(modified).is_ok_and(|age| age >= KEEP)
}

/// Startup: remove saved outputs nobody has read in a week, and any folder left
/// empty by that. Bounded and quiet, like the scratch sweep beside it.
pub(crate) fn prune(st: &AppState) {
    let Some(root) = crate::sandbox::data_folder(st).map(|dir| dir.join(FOLDER)) else { return };
    let now = SystemTime::now();
    let Ok(conversations) = std::fs::read_dir(&root) else { return };
    for conversation in conversations.flatten() {
        let path: PathBuf = conversation.path();
        if !path.is_dir() { continue; }
        if let Ok(files) = std::fs::read_dir(&path) {
            for file in files.flatten() {
                let file_path = file.path();
                if !file_path.is_file() { continue; }
                let stale = std::fs::metadata(&file_path)
                    .and_then(|meta| meta.modified())
                    .is_ok_and(|modified| is_stale(modified, now));
                if stale {
                    let _ = std::fs::remove_file(&file_path);
                }
            }
        }
        // A folder with nothing left in it is the wave's own remains.
        if std::fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_none()) {
            let _ = std::fs::remove_dir(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_returned_whole() {
        let capped = head_and_tail("hello", HEAD_BYTES, TAIL_BYTES);
        assert_eq!(capped.text, "hello");
        assert!(!capped.truncated);
        assert_eq!(capped.bytes, 5);
    }

    #[test]
    fn a_long_output_keeps_its_first_and_last_part_and_says_how_much_is_missing() {
        let text = format!("{}MIDDLE{}", "a".repeat(100), "z".repeat(100));
        let capped = head_and_tail(&text, 60, 40);
        assert!(capped.truncated);
        assert_eq!(capped.bytes, 206, "100 + MIDDLE + 100 bytes");
        assert!(capped.text.starts_with(&"a".repeat(60)), "the head is the first 60 bytes");
        assert!(capped.text.ends_with(&"z".repeat(40)), "the tail is the last 40 bytes");
        assert!(capped.text.contains("106 bytes left out"), "the marker counts what was dropped");
        assert!(!capped.text.contains("MIDDLE"), "the middle is the part that is left out");
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        // A three-byte character straddling the cut: the head stops before it and
        // the tail starts after it, so both are still valid UTF-8.
        let text = format!("{}{}", "a".repeat(9), "€".repeat(40));
        let capped = head_and_tail(&text, 10, 10);
        assert!(capped.truncated);
        assert!(capped.text.is_char_boundary(0));
        assert!(std::str::from_utf8(capped.text.as_bytes()).is_ok());
        assert!(!capped.text.contains('\u{fffd}'));
    }

    #[test]
    fn a_saved_output_is_stale_after_a_week_of_no_use() {
        let now = SystemTime::now();
        assert!(!is_stale(now, now));
        assert!(!is_stale(now - Duration::from_secs(6 * 24 * 60 * 60), now));
        assert!(is_stale(now - KEEP - Duration::from_secs(60), now));
        assert!(!is_stale(now + Duration::from_secs(60), now));
    }
}
