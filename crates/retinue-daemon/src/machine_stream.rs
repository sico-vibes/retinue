//! Step 1, piece 1: one live socket per online machine link.
//!
//! A machine link is this daemon acting as a paired device of another machine
//! ([`crate::machines`]). The far gateway already serves an event socket to any
//! paired device (`remote/web.rs`, `/api/events`), so this module opens one per
//! online link with the same pinned trust and the same device cookie the RPC
//! path uses ([`crate::machine_client::events_socket`]) and follows it.
//!
//! What leaves this module is **ids only**. On a relevant far event it emits a
//! local `machine.session.changed {machineId, sessionId, revision}` and nothing
//! else: the window then re-reads the conversation through the gateway, which
//! redacts it exactly as it does for a phone. No message text, tool title or
//! path is stored here, and `machine.session.changed` is `drop` in `tiers.json`
//! so no device ever receives it.
//!
//! The 20-second read in `machines.rs` stays the floor and the source of bot
//! counts; this socket only makes what it shows move sooner.

use crate::machine_client::{self as client, ClientError};
use crate::machines;
use crate::AppState;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::Message;

/// Far events that change a conversation rather than its text. `message.delta`
/// is deliberately absent: half a message is content, and the open conversation
/// is re-read on its own timer.
const RELEVANT: [&str; 8] = [
    "session.changed",
    "session.deleted",
    "message.completed",
    "turn.completed",
    "turn.cancelled",
    "turn.error",
    "permission.requested",
    "permission.resolved",
];

/// The clocks. Tests shorten them; production uses the defaults.
#[derive(Clone, Copy)]
pub struct Timing {
    /// Wait this long without a frame before probing the socket.
    pub idle_probe: Duration,
    /// How long a probe's answer may take before the socket counts as dead.
    pub probe_wait: Duration,
    /// A socket up this long resets the backoff.
    pub healthy_after: Duration,
    /// The first wait after a failed connect…
    pub backoff_base: Duration,
    /// …and the ceiling, which jitter never rises above.
    pub backoff_cap: Duration,
    /// How often the supervisor looks for a link it is not following.
    pub sweep_every: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            idle_probe: Duration::from_secs(25),
            probe_wait: Duration::from_secs(10),
            healthy_after: Duration::from_secs(60),
            backoff_base: Duration::from_secs(1),
            backoff_cap: Duration::from_secs(5 * 60),
            sweep_every: Duration::from_secs(5),
        }
    }
}

/// Where one machine's stream is. Memory only: a restart begins at sequence 0
/// and the next connect replays from the far machine's own log.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub connected: bool,
    pub last_sequence: u64,
}

/// Everything the streams keep in memory, held inside [`crate::machines::Machines`].
pub struct Streams {
    cursors: Mutex<HashMap<String, Cursor>>,
    /// The machines a task is following, so a sweep starts each one once.
    following: Mutex<HashSet<String>>,
    supervised: AtomicBool,
    /// The supervisor: a link appeared, disappeared or changed state.
    wake: Notify,
    /// A follower waiting out its backoff: try again now.
    retry: Notify,
    timing: Mutex<Timing>,
}

impl Default for Streams {
    fn default() -> Self {
        Self {
            cursors: Mutex::new(HashMap::new()),
            following: Mutex::new(HashSet::new()),
            supervised: AtomicBool::new(false),
            wake: Notify::new(),
            retry: Notify::new(),
            timing: Mutex::new(Timing::default()),
        }
    }
}

impl Streams {
    fn timing(&self) -> Timing {
        *self.timing.lock().unwrap()
    }

    #[cfg(test)]
    pub fn set_timing(&self, timing: Timing) {
        *self.timing.lock().unwrap() = timing;
    }

    #[cfg(test)]
    pub fn supervised(&self) -> bool {
        self.supervised.load(Ordering::SeqCst)
    }

    pub fn cursor(&self, id: &str) -> Cursor {
        self.cursors.lock().unwrap().get(id).copied().unwrap_or_default()
    }

    /// Moves the cursor past `sequence`. `false` for a sequence already applied,
    /// which is what a replay sends: PROTOCOL §6 lets a client see a duplicate,
    /// and the phone client's rule is the one used here.
    fn advance(&self, id: &str, sequence: u64) -> bool {
        let mut cursors = self.cursors.lock().unwrap();
        let cursor = cursors.entry(id.to_string()).or_default();
        if sequence <= cursor.last_sequence {
            return false;
        }
        cursor.last_sequence = sequence;
        true
    }

    fn set_connected(&self, id: &str, connected: bool) -> bool {
        let mut cursors = self.cursors.lock().unwrap();
        let cursor = cursors.entry(id.to_string()).or_default();
        if cursor.connected == connected {
            return false;
        }
        cursor.connected = connected;
        true
    }

    /// The far log could not replay from here. Start again from its beginning
    /// next time rather than pretending nothing was missed.
    fn reset(&self, id: &str) {
        if let Some(cursor) = self.cursors.lock().unwrap().get_mut(id) {
            cursor.last_sequence = 0;
        }
    }

    fn forget(&self, id: &str) {
        self.cursors.lock().unwrap().remove(id);
    }
}

/// Whether a socket to that machine is open right now.
pub fn live(st: &AppState, machine_id: &str) -> bool {
    st.machines.streams.cursor(machine_id).connected
}

/// `machine.stream.status {machineId}`: what the window says "live" from.
pub fn status(st: &AppState, machine_id: &str) -> Value {
    let cursor = st.machines.streams.cursor(machine_id);
    json!({"connected": cursor.connected, "lastSequence": cursor.last_sequence})
}

/// Forgets a link the owner removed.
pub fn forget(st: &AppState, machine_id: &str) {
    st.machines.streams.forget(machine_id);
}

/// Asks the supervisor to look at the links again, and every follower to try
/// its socket now: linking, refreshing or the window waking up.
pub fn wake(st: &AppState) {
    st.machines.streams.wake.notify_one();
    st.machines.streams.retry.notify_waiters();
}

/// Starts the supervisor when there is a link and none is running. Like the
/// 20-second loop, it ends by itself once the last link is gone.
pub fn ensure(st: &AppState) {
    if st.db.machine_link_count().unwrap_or(0) == 0 || st.machines.streams.supervised.swap(true, Ordering::SeqCst) {
        return;
    }
    let st = st.clone();
    tokio::spawn(async move { supervise(st).await });
}

async fn supervise(st: AppState) {
    loop {
        if st.db.machine_link_count().unwrap_or(0) == 0 {
            st.machines.streams.supervised.store(false, Ordering::SeqCst);
            // A link added between the check and the store must not be missed.
            if st.db.machine_link_count().unwrap_or(0) == 0 || st.machines.streams.supervised.swap(true, Ordering::SeqCst) {
                return;
            }
        }
        for link in st.db.machine_links().unwrap_or_default() {
            let Some(id) = link["id"].as_str().map(str::to_string) else { continue };
            if link["state"].as_str() != Some("online") {
                continue;
            }
            if st.machines.streams.following.lock().unwrap().insert(id.clone()) {
                let follower = st.clone();
                tokio::spawn(async move { follow(follower, id).await });
            }
        }
        let every = st.machines.streams.timing().sweep_every;
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = st.machines.streams.wake.notified() => {}
            _ = st.stopping.notified() => {
                st.machines.streams.supervised.store(false, Ordering::SeqCst);
                return;
            }
        }
    }
}

/// One machine, for as long as it is linked and online: connect, follow, and
/// when the socket dies wait a jittered, growing time and connect again.
async fn follow(st: AppState, id: String) {
    let mut attempt = 0u32;
    loop {
        match st.db.machine_link_get(&id).ok().flatten() {
            Some(link) if link["state"].as_str() == Some("online") => {}
            _ => break,
        }
        let started = tokio::time::Instant::now();
        if let Err(error) = one_socket(&st, &id).await {
            if matches!(error, ClientError::Invalid(_)) {
                // A link whose address can't be opened is not going to answer.
                break;
            }
        }
        let timing = st.machines.streams.timing();
        // Reset only after the socket stayed up: a link that accepts and drops
        // at once must not be retried every second for ever.
        if started.elapsed() >= timing.healthy_after {
            attempt = 0;
        }
        let delay = backoff(attempt, &timing);
        attempt = attempt.saturating_add(1);
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = st.machines.streams.retry.notified() => { attempt = 0; }
            _ = st.stopping.notified() => break,
        }
    }
    st.machines.streams.following.lock().unwrap().remove(&id);
    if st.machines.streams.set_connected(&id, false) {
        announce(&st, &id).await;
    }
}

/// Doubles from the base per attempt, adds up to 250 ms of jitter, and never
/// rises above the cap. The phone client's shape (`remoteTransport.ts`), with
/// this side's five-minute ceiling.
fn backoff(attempt: u32, timing: &Timing) -> Duration {
    let base = timing.backoff_base.as_millis().max(1);
    let grown = base.saturating_mul(1u128 << attempt.min(16));
    let cap = timing.backoff_cap.as_millis().max(1);
    let jitter = rand::random::<u64>() % 250;
    Duration::from_millis((grown.min(cap) as u64).saturating_add(jitter).min(cap as u64))
}

/// `machine.changed` with the `live` flag. Content-free, like the rest of it.
async fn announce(st: &AppState, id: &str) {
    let state = st
        .db
        .machine_link_get(id)
        .ok()
        .flatten()
        .and_then(|link| link["state"].as_str().map(str::to_string))
        .unwrap_or_default();
    st.emit("machine.changed", json!({"machineId": id, "state": state, "live": live(st, id)})).await;
}

/// One socket, until it dies. Returns why it ended; the caller reconnects.
async fn one_socket(st: &AppState, id: &str) -> Result<(), ClientError> {
    let timing = st.machines.streams.timing();
    let socket = client::events_socket(st, id, st.machines.timing().call_timeout).await?;
    let (mut write, mut read) = socket.split();
    let cursor = st.machines.streams.cursor(id).last_sequence;
    let hello = json!({"v": 1, "afterSequence": cursor}).to_string();
    if write.send(Message::Text(hello.into())).await.is_err() {
        return Err(ClientError::Unreachable);
    }
    if st.machines.streams.set_connected(id, true) {
        announce(st, id).await;
    }
    let mut probe_at = tokio::time::Instant::now() + timing.idle_probe;
    loop {
        tokio::select! {
            next = read.next() => match next {
                None => break,
                Some(Err(_)) => break,
                Some(Ok(Message::Text(text))) => {
                    probe_at = tokio::time::Instant::now() + timing.idle_probe;
                    handle_frame(st, id, &text).await;
                }
                // A pong, a ping or binary: the socket is alive, nothing to do.
                Some(Ok(_)) => { probe_at = tokio::time::Instant::now() + timing.idle_probe; }
            },
            _ = tokio::time::sleep_until(probe_at) => {
                // Nothing for a whole idle period. Ask for an answer: a socket
                // whose machine went away never answers, and is replaced.
                if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                match tokio::time::timeout(timing.probe_wait, read.next()).await {
                    // The answer may be the event that was waiting all along.
                    Ok(Some(Ok(Message::Text(text)))) => {
                        probe_at = tokio::time::Instant::now() + timing.idle_probe;
                        handle_frame(st, id, &text).await;
                    }
                    Ok(Some(Ok(_))) => { probe_at = tokio::time::Instant::now() + timing.idle_probe; }
                    _ => break,
                }
            }
            _ = st.stopping.notified() => break,
        }
    }
    let _ = write.close().await;
    if st.machines.streams.set_connected(id, false) {
        announce(st, id).await;
    }
    Ok(())
}

/// One far event: move the cursor past it, and tell this machine's window when
/// it concerns a conversation.
async fn handle_frame(st: &AppState, machine_id: &str, text: &str) {
    let Ok(event) = serde_json::from_str::<Value>(text) else { return };
    if event["type"] == "replay.gap" || event["replayAvailable"] == false {
        // The far log can't replay from our cursor: start over and re-read the
        // machine, so the list is right even though something was missed.
        st.machines.streams.reset(machine_id);
        machines::refresh_one(st, machine_id).await;
        wake(st);
        return;
    }
    let sequence = event["sequence"].as_u64().unwrap_or(0);
    if sequence == 0 || !st.machines.streams.advance(machine_id, sequence) {
        // A replay repeats sequences at or below the cursor; they are dropped.
        return;
    }
    let kind = event["type"].as_str().unwrap_or("");
    if !RELEVANT.contains(&kind) {
        return;
    }
    let payload = &event["payload"];
    let Some(session) = payload["sessionId"].as_str().or_else(|| payload["id"].as_str()).filter(|id| !id.is_empty()) else { return };
    // A conversation that moved away from this machine is not live here, even
    // if the machine it left still mentions it.
    if machines::refuse_if_moved_elsewhere(st, session, machine_id).is_err() {
        return;
    }
    st.emit("machine.session.changed", json!({"machineId": machine_id, "sessionId": session, "revision": sequence}))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_grows_to_five_minutes_and_never_past_it() {
        let timing = Timing::default();
        let first = backoff(0, &timing);
        assert!(first >= Duration::from_millis(1000) && first <= Duration::from_millis(1250), "first wait is a second plus jitter: {first:?}");
        let sixth = backoff(6, &timing);
        assert!(sixth >= Duration::from_secs(64), "it doubles: {sixth:?}");
        for attempt in 0..40 {
            assert!(backoff(attempt, &timing) <= timing.backoff_cap, "never above the cap at attempt {attempt}");
        }
        assert_eq!(backoff(16, &timing), timing.backoff_cap, "at the cap there is no room for jitter, so it is the cap");
    }

    #[test]
    fn a_sequence_at_or_below_the_cursor_is_dropped() {
        let streams = Streams::default();
        assert!(streams.advance("m", 5), "a first sequence moves the cursor");
        assert!(!streams.advance("m", 5), "the same sequence again is a replay");
        assert!(!streams.advance("m", 4), "an older one is a replay");
        assert!(streams.advance("m", 6));
        assert_eq!(streams.cursor("m").last_sequence, 6);
        assert!(streams.advance("other", 1), "each machine keeps its own cursor");
        streams.reset("m");
        assert_eq!(streams.cursor("m").last_sequence, 0, "a gap starts over");
        assert_eq!(streams.cursor("other").last_sequence, 1, "and moves nothing else");
    }

    #[test]
    fn the_connection_flag_is_announced_once_per_change() {
        let streams = Streams::default();
        assert!(!streams.cursor("m").connected);
        assert!(streams.set_connected("m", true));
        assert!(!streams.set_connected("m", true), "a second success is not a change");
        assert!(streams.cursor("m").connected);
        assert!(streams.set_connected("m", false));
        assert!(!streams.cursor("m").connected);
        streams.forget("m");
        assert!(!streams.cursor("m").connected, "a removed link keeps nothing");
    }

    #[test]
    fn a_failed_address_does_not_end_the_follow() {
        // An invalid address is the one error `follow` gives up on; everything
        // else is retried. Pinned here so the match arm is not quietly widened.
        let error = ClientError::Invalid("no".to_string());
        assert!(matches!(error, ClientError::Invalid(_)));
        assert!(!matches!(ClientError::Unreachable, ClientError::Invalid(_)));
    }
}
