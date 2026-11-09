//! The completion peek: a short banner when another bot's turn finishes
//! (`apps/desktop/src/ui/companionPeek.ts`).
//!
//! The trigger and the coalescing are pure so they can be table-tested; the view
//! owns only the timer and the copy.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

/// How long a peek stays up before the normal status line returns.
pub const PEEK_VISIBLE_MS: u64 = 4_000;
/// At most one peek in this window; later finishes fold into the one on screen.
pub const PEEK_WINDOW_MS: u64 = 10_000;
/// Session states that mean a turn is in flight.
const BUSY_STATES: [&str; 2] = ["working", "starting"];
/// Session states that mean the turn is done and the bot is waiting quietly.
const FINISHED_STATES: [&str; 2] = ["idle", "completed"];

/// One conversation's state at a moment, as far as a peek cares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeekSession {
    pub id: String,
    pub state: Option<String>,
}

impl PeekSession {
    pub fn new(id: impl Into<String>, state: Option<&str>) -> Self {
        Self { id: id.into(), state: state.map(str::to_owned) }
    }

    fn word(&self) -> String {
        self.state.as_deref().unwrap_or("").to_lowercase()
    }
}

/// The bots that finished a turn between two snapshots. The focused bot is
/// never included (its mood already shows it), and neither is a failed turn
/// (that uses the error mood).
pub fn detect_finished(previous: &[PeekSession], next: &[PeekSession], focused: Option<&str>) -> Vec<String> {
    let before: Vec<(&str, String)> = previous.iter().map(|item| (item.id.as_str(), item.word())).collect();
    let mut finished = Vec::new();
    for item in next {
        if focused == Some(item.id.as_str()) {
            continue;
        }
        let Some((_, was)) = before.iter().find(|(id, _)| *id == item.id.as_str()) else { continue };
        if !BUSY_STATES.contains(&was.as_str()) {
            continue;
        }
        if FINISHED_STATES.contains(&item.word().as_str()) {
            finished.push(item.id.clone());
        }
    }
    finished
}

/// What the companion shows now: the new peek, the one on screen with the new
/// bots folded in (within the window), or the previous one when nothing
/// finished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peek {
    /// The finished bots, in the order they were first seen.
    pub ids: Vec<String>,
    /// When the current peek window opened, in milliseconds since the app began.
    pub at: u64,
}

pub fn next_peek(previous: Option<Peek>, finished: &[String], now_ms: u64, window_ms: u64) -> Option<Peek> {
    if finished.is_empty() {
        return previous;
    }
    if let Some(previous) = previous.as_ref().filter(|peek| now_ms.saturating_sub(peek.at) < window_ms) {
        let mut ids = previous.ids.clone();
        for id in finished {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        return Some(Peek { ids, at: previous.at });
    }
    Some(Peek { ids: finished.to_vec(), at: now_ms })
}

/// "Claude finished." for one, "3 bots finished." for several.
pub fn peek_text(names: &[String], plural_noun: &str) -> String {
    match names.len() {
        0 => String::new(),
        1 => format!("{} finished.", names[0]),
        count => format!("{count} {plural_noun} finished."),
    }
}

/// Whether the peek is still on screen at `now_ms`.
pub fn peek_visible(peek: &Peek, now_ms: u64) -> bool {
    now_ms >= peek.at && now_ms - peek.at < PEEK_VISIBLE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(rows: &[(&str, &str)]) -> Vec<PeekSession> {
        rows.iter().map(|(id, state)| PeekSession::new(*id, Some(*state))).collect()
    }

    #[test]
    fn only_a_busy_bot_that_finished_earns_a_peek() {
        let previous = states(&[("a", "working"), ("b", "idle"), ("c", "starting")]);
        let next = states(&[("a", "idle"), ("b", "idle"), ("c", "working")]);
        assert_eq!(detect_finished(&previous, &next, None), vec!["a".to_string()]);
        // The focused bot's mood already says so.
        assert!(detect_finished(&previous, &next, Some("a")).is_empty());
        // A failed turn is the error mood, not a peek.
        assert!(detect_finished(&previous, &states(&[("a", "error")]), None).is_empty());
        // A bot we have never seen working does not peek.
        assert!(detect_finished(&previous, &states(&[("z", "idle")]), None).is_empty());
    }

    #[test]
    fn later_finishes_fold_into_the_peek_on_screen() {
        let first = next_peek(None, &["a".to_owned()], 1_000, PEEK_WINDOW_MS).unwrap();
        assert_eq!(first.at, 1_000);
        let folded = next_peek(Some(first.clone()), &["b".to_owned()], 5_000, PEEK_WINDOW_MS).unwrap();
        assert_eq!(folded.ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(folded.at, 1_000, "the window does not restart");
        // After the window, a finish starts a fresh peek.
        let fresh = next_peek(Some(folded), &["c".to_owned()], 12_000, PEEK_WINDOW_MS).unwrap();
        assert_eq!(fresh.ids, vec!["c".to_string()]);
        assert_eq!(fresh.at, 12_000);
        // Nothing finished: nothing changes.
        assert_eq!(next_peek(Some(first.clone()), &[], 20_000, PEEK_WINDOW_MS), Some(first));
    }

    #[test]
    fn the_peek_copy_names_one_or_counts_several() {
        assert_eq!(peek_text(&[], "bots"), "");
        assert_eq!(peek_text(&["Claude".to_owned()], "bots"), "Claude finished.");
        assert_eq!(peek_text(&["Claude".to_owned(), "Gogo".to_owned(), "Sona".to_owned()], "bots"), "3 bots finished.");
    }

    #[test]
    fn a_peek_is_up_for_four_seconds() {
        let peek = Peek { ids: vec!["a".into()], at: 1_000 };
        assert!(peek_visible(&peek, 1_000));
        assert!(peek_visible(&peek, 4_999));
        assert!(!peek_visible(&peek, 5_000));
    }
}
