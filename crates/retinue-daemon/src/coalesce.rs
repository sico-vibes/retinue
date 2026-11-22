//! Streamed assistant text, held for one window before it is written and sent
//! (21 November 2026, performance item 1).
//!
//! Every adapter reports its provider's chunks as `AgentEvent::AssistantDelta`;
//! in steady state that is one chunk per token. Writing each one costs a row
//! update and an event insert, each its own commit on a `synchronous=FULL`
//! connection (2.6 to 11.6 ms on the owner's data, `lessons.md` 1 November
//! 2026), and each one wakes the window with a whole-app render. The daemon now
//! holds a turn's assistant text in [`DeltaBuffer`] and flushes it as one row
//! append and one `message.delta`:
//!
//! * when [`DELTA_FLUSH_INTERVAL`] passes since the buffer opened (40 ms, below
//!   the 50 ms the owner accepted as the most a crash may lose),
//! * when the buffered text reaches [`DELTA_FLUSH_BYTES`] (8 KiB, so one event
//!   never carries an unbounded string),
//! * and in `forward_events_with_timeouts` before any other event of the same
//!   session, before a turn's terminal event, before a failure and at the end,
//!   so replay order and the stored transcript are unchanged.
//!
//! A crash may therefore lose up to 40 ms of a streamed reply; the row and
//! `message.completed` stay the authority (owner decision, 21 November 2026).
//! Nothing here logs or stores anything itself.

use std::time::{Duration, Instant};

/// The longest streamed chunk waits in memory. Pandacode flushes at 33 ms, T3
/// Code at 50 ms; 40 ms sits between them, at 25 UI updates a second (above the
/// ~20 a second a person reads as a smooth stream) and under the 50 ms the
/// owner approved as the most a crash may lose.
pub const DELTA_FLUSH_INTERVAL: Duration = Duration::from_millis(40);

/// A flush also happens as soon as the buffered text reaches this size, so one
/// `message.delta` never carries more than a readable chunk of a fast stream.
pub const DELTA_FLUSH_BYTES: usize = 8 * 1024;

/// One turn's unflushed assistant text.
pub struct DeltaBuffer {
    turn: String,
    text: String,
    opened: Instant,
}

impl DeltaBuffer {
    pub fn new(turn: &str) -> Self {
        Self { turn: turn.to_owned(), text: String::new(), opened: Instant::now() }
    }

    /// The turn the buffered text belongs to (empty when the session had no
    /// active turn, exactly as `message.delta` reported it before).
    pub fn turn(&self) -> &str {
        &self.turn
    }

    pub fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Whether the buffer must flush now: it holds text and either the interval
    /// has closed or the size cap was reached. An empty buffer never flushes.
    pub fn is_due(&self, now: Instant) -> bool {
        !self.text.is_empty()
            && (self.text.len() >= DELTA_FLUSH_BYTES
                || now.saturating_duration_since(self.opened) >= DELTA_FLUSH_INTERVAL)
    }

    /// How long until the interval closes, for the event loop's own wait.
    /// `Duration::ZERO` when it is already due.
    pub fn time_to_due(&self, now: Instant) -> Duration {
        DELTA_FLUSH_INTERVAL.saturating_sub(now.saturating_duration_since(self.opened))
    }

    /// Takes the text to write, leaving the buffer empty.
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hundred_small_chunks_become_one_flush_and_the_size_cap_still_closes_one() {
        let mut buffer = DeltaBuffer::new("turn");
        let opened = Instant::now();
        // 1,000 chunks of 4 bytes: under the size cap, and no time passes in a
        // test, so the only flush is the caller's final one.
        for _ in 0..1_000 {
            buffer.push("abcd");
        }
        assert!(!buffer.is_due(opened), "4,000 bytes inside the interval is one flush, not more");
        assert_eq!(buffer.take().len(), 4_000);
        assert!(!buffer.is_due(opened), "an empty buffer is never due");

        let mut buffer = DeltaBuffer::new("turn");
        let opened = Instant::now();
        buffer.push(&"x".repeat(DELTA_FLUSH_BYTES));
        assert!(buffer.is_due(opened), "the size cap closes the buffer at once");
    }

    #[test]
    fn the_interval_closes_the_buffer_after_forty_milliseconds() {
        let mut buffer = DeltaBuffer::new("turn");
        buffer.push("one token");
        let opened = Instant::now();
        assert!(!buffer.is_due(opened));
        assert!(!buffer.is_due(opened + DELTA_FLUSH_INTERVAL - Duration::from_millis(1)));
        assert!(buffer.is_due(opened + DELTA_FLUSH_INTERVAL));
        assert_eq!(buffer.time_to_due(opened + DELTA_FLUSH_INTERVAL), Duration::ZERO);
        assert_eq!(DeltaBuffer::new("turn").turn(), "turn");
    }
}
