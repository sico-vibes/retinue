//! The "now" line a bot writes about what it is doing (wave 2, nod idea 2).
//!
//! Our working line is inferred from the running tool ("Running npm test"),
//! which is honest but mechanical. This module adds one tool, `set_working_note`,
//! so a bot can say in its own words what the step is for, and one event,
//! `session.note`, so the window can show it ahead of the inferred label.
//!
//! Rails, all of them the owner's rules rather than preferences:
//!
//! - The text is model output, so it is bounded (140 characters), stripped of
//!   control characters, refused when it looks like a credential, and never
//!   logged. It is never stored on a turn row and never reaches a phone
//!   (`session.note` is `drop` in `tiers.json`).
//! - Only a running turn may write one: the note is about what is happening now,
//!   and a note written outside a turn would be a status the daemon cannot clear.
//! - The daemon clears the line itself when the turn ends (one empty note), so a
//!   stale note cannot outlive its turn.
//! - It can never claim completion: the turn's own state and the transcript's own
//!   rows are what say a turn finished.

use crate::AppState;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::LazyLock, sync::Mutex};

/// Longest working note, in characters. nod uses the same bound for its live line.
pub const MAX_NOTE_CHARS: usize = 140;

/// Which turn holds a live note, per conversation. In memory only: a note is
/// about the turn running now, and a restarted daemon has no running turns.
/// It exists so the clear at turn end is emitted only when a note was written,
/// instead of one empty event per turn.
static LIVE_NOTES: LazyLock<Mutex<HashMap<String, (String, String)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The MCP tool schema, offered with the other team tools.
pub fn tool_schema() -> Value {
    json!({
        "name": "set_working_note",
        "description": "Replace the one-line status the owner sees while this turn runs, with what you are doing and why (for example \"Checking the release gate before merging\"). One short sentence, no completion claims: the turn's own state says when it is done. Call it when the step changes; stay quiet when it does not.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "One sentence, up to 140 characters, in plain sentence case."}
            },
            "required": ["text"],
            "additionalProperties": false
        }
    })
}

/// Trims, flattens and bounds a note. Control characters (including newlines)
/// become single spaces, so the line stays one line in the window.
fn clean_note(raw: &str) -> Result<String, String> {
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = flat.chars().filter(|ch| !ch.is_control()).collect();
    let text = text.trim();
    if text.is_empty() { return Err("A working note must be 1 to 140 characters of plain text.".into()); }
    if text.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("A working note must be {MAX_NOTE_CHARS} characters or fewer."));
    }
    Ok(text.to_owned())
}

/// `set_working_note`: records the line for the turn running in this conversation.
pub async fn set_note(st: &AppState, session_id: &str, args: &Value) -> Result<String, String> {
    let text = clean_note(args["text"].as_str().unwrap_or(""))?;
    if retinue_security::contains_secret_marker(&text) {
        return Err("That note looks like a credential; working notes cannot carry secrets.".into());
    }
    let Some(turn) = st.active_turns.lock().await.get(session_id).cloned() else {
        return Err("There is no turn running in this conversation to note anything for.".into());
    };
    LIVE_NOTES.lock().unwrap_or_else(|poison| poison.into_inner()).insert(session_id.to_owned(), (turn.clone(), text.clone()));
    st.emit("session.note", json!({"sessionId": session_id, "turnId": turn, "text": text})).await;
    Ok(format!("Noted: {text}"))
}

/// The empty note that clears the line, emitted once when a turn's event stream
/// ends, which is the one point every terminal path (completed, failed,
/// cancelled, interrupted) passes through. Nothing is emitted when the turn wrote
/// no note, so a quiet turn adds no event.
pub async fn announce_clear(st: &AppState, session_id: &str) {
    let live = LIVE_NOTES.lock().unwrap_or_else(|poison| poison.into_inner()).remove(session_id);
    if let Some((turn, _text)) = live {
        st.emit("session.note", json!({"sessionId": session_id, "turnId": turn, "text": ""})).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_one_bounded_line() {
        assert_eq!(clean_note("  Checking the gate\nbefore merging  ").unwrap(), "Checking the gate before merging");
        assert!(clean_note("   ").is_err());
        assert!(clean_note(&"x".repeat(MAX_NOTE_CHARS)).is_ok());
        assert!(clean_note(&"x".repeat(MAX_NOTE_CHARS + 1)).is_err());
        // A tab and a carriage return never survive as layout.
        assert_eq!(clean_note("a\tb\rc").unwrap(), "a b c");
    }
}
