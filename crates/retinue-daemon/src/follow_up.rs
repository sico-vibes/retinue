//! Follow-up prompt suggestions (21 November 2026).
//!
//! After a turn completes in an attended conversation, the daemon may ask the
//! conversation's own provider CLI, in a separate one-shot process, for the next
//! prompt the owner would plausibly type. The answer becomes greyed ghost text in
//! the composer. It is held in memory only: never in the database, a log, a
//! backup or a push. The contract is in `docs/ipc-contract.md` ("Follow-up
//! suggestions") and the rule is in `docs/reference/invariants.md`.
//!
//! # How each runtime answers, checked against the installed CLIs on 21 November 2026
//!
//! * **Claude** runs `claude -p --output-format json --permission-mode default
//!   --no-session-persistence` with the prompt on stdin: the same one-shot call
//!   the Claude adapter's model-catalog probe already makes. The cheapest model
//!   the runtime reports (a Haiku alias) is named with `--model`.
//! * **Codex** runs `codex exec --ephemeral --sandbox read-only
//!   --skip-git-repo-check -C <temp> --output-last-message <temp> -` with the
//!   prompt on stdin. `--ephemeral` keeps no session files, and the CLI's own
//!   `--help` documents reading the prompt from stdin when `-` is named.
//! * **OpenCode**, the other ACP providers and **Pi** get no suggestion.
//!   OpenCode's `run` has no stdin form, and putting the prompt in `argv` would
//!   break Retinue's rule that a prompt reaches an agent through stdin or its
//!   own protocol. ACP and Pi have no clean one-shot side session either, so
//!   they answer nothing rather than open a second conversation.
//!
//! Nothing here logs the owner's message, the reply or the suggestion.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The suggestion is one line of at most this many characters.
pub const MAX_CHARS: usize = 200;
/// How long the one-shot side request may take before it is stopped and dropped.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// How much of the owner's last message and of the last reply are sent. Only
/// those two, trimmed, ever leave the machine for this call.
const USER_CAP: usize = 2_000;
const REPLY_CAP: usize = 4_000;
/// A quota window at or above this percentage counts as near the limit.
const NEAR_LIMIT_PERCENT: u64 = 90;
/// What a command may print into memory.
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// The setting. Absent means on; only an explicit `false` turns it off.
pub const SETTING: &str = "chat.followUpSuggestions";

/// What the provider is asked. It names the two parts and the reply's shape,
/// and nothing else: no project, no instructions, no transcript.
const INSTRUCTIONS: &str = "\
You write one follow-up message for the person using a desktop coding app.
Below are the last message they sent and the assistant's last reply.
Write the single next message the person would plausibly type, in their voice:
- plain text on one line, at most 200 characters;
- the same language they used;
- no quotes, no label, no explanation, no markdown;
- a bare acknowledgement such as \"thanks\" is not useful.
If nothing sensible comes next, answer with the single word NONE.";

/// The suggestion held for one conversation.
struct Held {
    turn_id: String,
    text: String,
}

/// One generation in flight, and the token that cancels it.
struct Running {
    epoch: u64,
    cancel: Arc<tokio::sync::Notify>,
}

#[derive(Default)]
struct Inner {
    /// The suggestion the composer may show, one per conversation.
    held: HashMap<String, Held>,
    /// The conversation with a generation running: one at most.
    running: HashMap<String, Running>,
    /// Bumped whenever the owner sends, dismisses or a new turn starts, so an
    /// answer that arrives afterwards is dropped rather than shown.
    epoch: HashMap<String, u64>,
}

/// The follow-up suggestions this daemon holds. Memory only: a restart forgets
/// every one, which is the point.
#[derive(Default)]
pub struct FollowUp {
    inner: Mutex<Inner>,
}

impl FollowUp {
    /// Takes the one in-flight slot for a conversation and returns its epoch and
    /// cancel token, or `None` when one is already running there.
    fn begin(&self, session: &str) -> Option<(u64, Arc<tokio::sync::Notify>)> {
        let mut inner = self.inner.lock().ok()?;
        if inner.running.contains_key(session) { return None; }
        let epoch = inner.epoch.get(session).copied().unwrap_or(0) + 1;
        inner.epoch.insert(session.to_owned(), epoch);
        let cancel = Arc::new(tokio::sync::Notify::new());
        inner.running.insert(session.to_owned(), Running { epoch, cancel: cancel.clone() });
        Some((epoch, cancel))
    }

    /// Whether the answer for `epoch` still belongs to the conversation: the
    /// owner has not sent, dismissed or started a turn since it was asked.
    fn is_current(&self, session: &str, epoch: u64) -> bool {
        let Ok(inner) = self.inner.lock() else { return false };
        inner.epoch.get(session).copied() == Some(epoch)
            && inner.running.get(session).map(|running| running.epoch) == Some(epoch)
    }

    /// Frees the in-flight slot when the generation that took it finishes.
    fn finish(&self, session: &str, epoch: u64) -> bool {
        let Ok(mut inner) = self.inner.lock() else { return false };
        let mine = inner.running.get(session).map(|running| running.epoch) == Some(epoch);
        if mine { inner.running.remove(session); }
        mine && inner.epoch.get(session).copied() == Some(epoch)
    }

    /// Holds an answer, replacing any older one for the conversation.
    fn hold(&self, session: &str, turn: &str, text: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.held.insert(session.to_owned(), Held { turn_id: turn.to_owned(), text });
        }
    }

    /// Bumps the epoch, cancels a running generation and returns whether a
    /// suggestion was held. The caller announces the clear.
    fn clear(&self, session: &str) -> bool {
        let (held, cancel) = {
            let Ok(mut inner) = self.inner.lock() else { return false };
            *inner.epoch.entry(session.to_owned()).or_insert(0) += 1;
            let held = inner.held.remove(session).is_some();
            let cancel = inner.running.remove(session).map(|running| running.cancel);
            (held, cancel)
        };
        if let Some(cancel) = cancel { cancel.notify_waiters(); }
        held
    }

    fn get(&self, session: &str) -> Option<(String, String)> {
        let inner = self.inner.lock().ok()?;
        inner.held.get(session).map(|held| (held.turn_id.clone(), held.text.clone()))
    }
}

/// The two RPCs (`session.suggestion.get`, `session.suggestion.dismiss`). Both
/// are desktop only and answer from memory; neither writes anything.
pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    let session = p["sessionId"].as_str().filter(|id| !id.trim().is_empty())
        .ok_or_else(|| derr("invalid_argument", "sessionId is required", StatusCode::BAD_REQUEST))?;
    match method {
        "session.suggestion.get" => Ok(match st.follow_up.get(session).filter(|_| enabled(st)) {
            Some((turn_id, text)) => json!({"turnId":turn_id,"text":text}),
            None => Value::Null,
        }),
        "session.suggestion.dismiss" => Ok(json!({"sessionId":session,"cleared":clear(st, session).await})),
        _ => Err(derr("unsupported", "method is not supported", StatusCode::NOT_FOUND)),
    }
}

/// The setting's value: on unless the owner stored `false`.
pub fn enabled(st: &AppState) -> bool {
    st.db.settings().map(|settings| settings[SETTING] != false).unwrap_or(true)
}

/// The owner sent a message (or steered one) or a new turn started: the ghost
/// text goes, a running answer is cancelled, and the window is told if it was
/// showing one.
pub async fn clear(st: &AppState, session: &str) -> bool {
    let held = st.follow_up.clear(session);
    if held { st.emit("session.suggestion.cleared", json!({"sessionId":session})).await; }
    held
}

/// Why no suggestion is produced for this conversation, or `None` when one may
/// be. The words are for tests and for a person reading the code; nothing here
/// is logged and none of them is shown to the owner.
pub fn skip_reason(st: &AppState, session: &str, summary: &Value, user_meta: &Value, now: Instant) -> Option<&'static str> {
    if !enabled(st) { return Some("off"); }
    let link = &summary["link"];
    // An automation, and a task a schedule started, are the same rule.
    if crate::is_unattended(link) { return Some("unattended"); }
    // A side conversation is a teammate's; the owner has no composer to fill.
    if link["kind"] == "side" { return Some("side"); }
    // A conversation a paired device started is answered on that device.
    if link["startedFrom"].is_object() { return Some("remote"); }
    // A message a chat channel delivered is not the owner typing here.
    if user_meta["kind"] == "channel" { return Some("channel"); }
    if summary["modelLock"]["planMode"] == true || link["approvalMode"] == "plan" { return Some("plan"); }
    if summary["state"].as_str() == Some("limited") || limit_still_ahead(summary) { return Some("limited"); }
    if st.db.pending_question_exists(session).unwrap_or(false) { return Some("question"); }
    if near_limit(st, summary) { return Some("near_limit"); }
    // Only the conversation the owner is looking at, as Claude Code does: a
    // conversation nobody has on screen gets no suggestion and costs nothing.
    if !st.attention.focused(session, now) { return Some("unfocused"); }
    None
}

/// A usage limit whose reset is still ahead.
fn limit_still_ahead(summary: &Value) -> bool {
    summary["limitedUntil"].as_str()
        .and_then(|until| chrono::DateTime::parse_from_rfc3339(until).ok())
        .is_some_and(|until| until.to_utc() > chrono::Utc::now())
}

/// Whether the last reading of this provider's quota is at or near a limit. An
/// unread or stale quota never blocks a suggestion.
fn near_limit(st: &AppState, summary: &Value) -> bool {
    let provider = summary["provider"].as_str().unwrap_or("");
    let Ok(snapshots) = st.db.quota_snapshots() else { return false };
    let Some(row) = snapshots.iter().find(|row| row["provider"].as_str() == Some(provider)) else { return false };
    let snapshot = &row["snapshot"];
    if snapshot["ordinaryUsageAllowed"] == false { return true; }
    snapshot["limits"].as_array().is_some_and(|limits| limits.iter().any(|limit| {
        ["primary", "secondary"].iter().any(|window| {
            limit[*window]["usedPercent"].as_u64().is_some_and(|used| used >= NEAR_LIMIT_PERCENT)
        })
    }))
}

/// Called after a completed turn. It decides quickly from the database and, when
/// a suggestion is due, answers it off the turn's own event loop.
pub async fn after_turn(st: &AppState, session: &str, turn: &str) {
    if !enabled(st) { return; }
    let Ok(summary) = st.db.session_event_summary(session) else { return };
    let Some((user, user_meta)) = st.db.session_latest_user_message(session).ok().flatten() else { return };
    if skip_reason(st, session, &summary, &user_meta, Instant::now()).is_some() { return; }
    let Some(reply) = st.db.turn_reply(session, turn).ok().flatten() else { return };
    let Some(runtime) = st.runtimes.read().await.iter().find(|runtime| runtime["id"] == summary["runtimeId"]).cloned() else { return };
    let Some(ask) = ask_for(st, &summary, &runtime, prompt_for(&user, &reply)) else { return };
    let user_message = st.db.session_latest_user_message_id(session).ok().flatten();
    let Some((epoch, cancel)) = st.follow_up.begin(session) else { return };
    let st = st.clone();
    let session = session.to_owned();
    let turn = turn.to_owned();
    tokio::spawn(async move {
        let answer = tokio::select! {
            _ = cancel.notified() => None,
            answer = answer_with(&st, &ask, TIMEOUT) => answer,
        };
        let Some((text, usage)) = answer else {
            st.follow_up.finish(&session, epoch);
            return;
        };
        // The answer is dropped unless nothing has moved since it was asked: a
        // new message, a dismissal or a new turn all bump the epoch.
        if !st.follow_up.is_current(&session, epoch) { return; }
        if !st.follow_up.finish(&session, epoch) { return; }
        if st.db.session_latest_user_message_id(&session).ok().flatten() != user_message { return; }
        if !enabled(&st) { return; }
        if let Some(usage) = usage { record_usage(&st, &session, &turn, &ask, usage); }
        st.follow_up.hold(&session, &turn, text.clone());
        // The text is never persisted: a content-free marker takes the sequence
        // number the event stream needs, and the suggestion itself is broadcast
        // live only. `docs/reference/invariants.md` says why.
        st.emit_live("session.suggestion.ready", json!({"sessionId":session}), "session.suggestion", json!({"sessionId":session,"turnId":turn,"text":text})).await;
    });
}

/// One one-shot side request, prepared from the runtime the conversation runs on.
pub(crate) struct Ask {
    provider: String,
    runtime_id: String,
    /// The bot, so the call runs with the same account environment a turn would.
    agent_id: String,
    model: Option<String>,
    program: OsString,
    args: Vec<String>,
    cwd: PathBuf,
    /// Where Codex writes its final message.
    output: Option<PathBuf>,
    prompt: String,
}

/// Builds the command for one runtime, or `None` when it has no clean way to
/// answer a one-off question (see the module note).
pub(crate) fn ask_for(st: &AppState, summary: &Value, runtime: &Value, prompt: String) -> Option<Ask> {
    let provider = summary["provider"].as_str()?;
    let spec = crate::runtime_from(runtime);
    if spec.executable.as_os_str().is_empty() { return None; }
    let agent_id = summary["agentId"].as_str().unwrap_or("").to_owned();
    let dir = std::env::temp_dir();
    match provider {
        "claude" => {
            let mut args = spec.args.clone();
            args.extend(["-p".to_owned(), "--output-format".to_owned(), "json".to_owned(), "--permission-mode".to_owned(), "default".to_owned(), "--no-session-persistence".to_owned()]);
            let model = cheapest_model(st, runtime);
            if let Some(model) = model.as_ref() { args.push("--model".to_owned()); args.push(model.clone()); }
            Some(Ask { provider: provider.to_owned(), runtime_id: spec.runtime_id, agent_id, model, program: spec.executable.into_os_string(), args, cwd: dir, output: None, prompt })
        }
        "codex" => {
            let out = dir.join(format!("retinue-suggestion-{}.txt", uuid::Uuid::new_v4()));
            let mut args = spec.args.clone();
            args.extend([
                "exec".to_owned(), "--ephemeral".to_owned(), "--sandbox".to_owned(), "read-only".to_owned(),
                "--skip-git-repo-check".to_owned(), "-C".to_owned(), dir.to_string_lossy().into_owned(),
                "--output-last-message".to_owned(), out.to_string_lossy().into_owned(), "-".to_owned(),
            ]);
            Some(Ask { provider: provider.to_owned(), runtime_id: spec.runtime_id, agent_id, model: None, program: spec.executable.into_os_string(), args, cwd: dir, output: Some(out), prompt })
        }
        _ => None,
    }
}

/// The cheapest model the runtime reports, from the provider's own catalog. Only
/// Claude names one: its cheapest family is Haiku, and an unknown catalog leaves
/// the CLI its own default rather than guessing a model id.
fn cheapest_model(st: &AppState, runtime: &Value) -> Option<String> {
    let served = crate::catalog::cached(st, runtime)?;
    served.value["models"].as_array()?.iter().find_map(|model| {
        let id = model["id"].as_str()?;
        id.to_ascii_lowercase().contains("haiku").then(|| id.to_owned())
    })
}

/// The side question: the two parts, trimmed and bounded, and nothing else.
fn prompt_for(user: &str, reply: &str) -> String {
    format!("{INSTRUCTIONS}\n\n<owner>\n{}\n</owner>\n\n<reply>\n{}\n</reply>\n", clip(user, USER_CAP), clip(reply, REPLY_CAP))
}

/// At most `limit` characters, cut on a character boundary.
fn clip(text: &str, limit: usize) -> &str {
    match text.char_indices().nth(limit) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

/// Runs one command with a time limit, its prompt on stdin, and its whole tree
/// stopped afterwards. No shell, no credential of its own: the CLI uses its own
/// stored sign-in, exactly as it does for a turn.
pub(crate) async fn answer_with(st: &AppState, ask: &Ask, limit: Duration) -> Option<(String, Option<Value>)> {
    let mut command = tokio::process::Command::new(&ask.program);
    retinue_process::prepare_command(&mut command);
    command
        .args(&ask.args)
        .current_dir(&ask.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Ok(options) = crate::exec_options_for_session(st, &json!({"agentId": ask.agent_id})) { command.envs(options.env); }
    let mut child = command.spawn().ok()?;
    let tree = retinue_process::ProcessTree::attach(&mut child).ok()?;
    let prompt = ask.prompt.clone();
    let work = async move {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes()).await;
            drop(stdin);
        }
        child.wait_with_output().await.ok()
    };
    let output = tokio::time::timeout(limit, work).await.ok().flatten();
    let _ = tree.terminate();
    let answer = output.filter(|output| output.status.success()).and_then(|output| interpret(ask, &output));
    if let Some(path) = ask.output.as_ref() { let _ = std::fs::remove_file(path); }
    answer
}

/// Reads the answer out of what the CLI printed, in its own shape.
fn interpret(ask: &Ask, output: &std::process::Output) -> Option<(String, Option<Value>)> {
    if ask.provider == "codex" {
        let text = std::fs::read_to_string(ask.output.as_ref()?).ok()?;
        return normalize_answer(&text).map(|text| (text, None));
    }
    let raw = String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(MAX_OUTPUT_BYTES)]).into_owned();
    let body = serde_json::from_str::<Value>(raw.trim()).ok();
    if body.as_ref().is_some_and(|body| body["is_error"] == true) { return None; }
    let usage = body.as_ref().and_then(|body| usage_of(body, ask.model.as_deref()));
    let text = body.as_ref().and_then(|body| body["result"].as_str()).map(str::to_owned).unwrap_or(raw);
    normalize_answer(&text).map(|text| (text, usage))
}

/// What the CLI reported for this call, in the shape the usage table keeps. A
/// CLI that reports no numbers gets no row: usage is never invented.
fn usage_of(body: &Value, model: Option<&str>) -> Option<Value> {
    let usage = &body["usage"];
    let number = |key: &str| usage[key].as_i64();
    let (input, output, read, written) = (number("input_tokens"), number("output_tokens"), number("cache_read_input_tokens"), number("cache_creation_input_tokens"));
    if [input, output, read, written].iter().all(Option::is_none) { return None; }
    let model = model.map(str::to_owned).or_else(|| body["modelUsage"].as_object().and_then(|models| models.keys().next().cloned()));
    Some(json!({"model":model,"inputTokens":input,"outputTokens":output,"cacheReadTokens":read,"cacheWriteTokens":written}))
}

/// One line, at most [`MAX_CHARS`], or nothing when the answer is empty or says
/// `NONE`. Quotes and a leading label are trimmed, because a model sometimes
/// wraps the message despite being asked not to.
fn normalize_answer(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut text = line.trim_start_matches(['"', '\'', '\u{201c}', '\u{2018}']).trim_end_matches(['"', '\'', '\u{201d}', '\u{2019}']);
    for label in ["next:", "suggestion:", "prompt:"] {
        if text.to_ascii_lowercase().starts_with(label) { text = text[label.len()..].trim(); }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.eq_ignore_ascii_case("none") || text.eq_ignore_ascii_case("no suggestion") { return None; }
    Some(clip(&text, MAX_CHARS).to_owned())
}

/// Counts the call's usage where the runtime reported it, against the turn it
/// followed: the spend is real, so it belongs to that conversation.
fn record_usage(st: &AppState, session: &str, turn: &str, ask: &Ask, usage: Value) {
    let row = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "runtimeId": ask.runtime_id,
        "sessionId": session,
        "turnId": turn,
        "provider": ask.provider,
        "model": usage["model"],
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "inputTokens": usage["inputTokens"],
        "outputTokens": usage["outputTokens"],
        "cacheReadTokens": usage["cacheReadTokens"],
        "cacheWriteTokens": usage["cacheWriteTokens"],
        "reasoningTokens": Value::Null,
        "source": "suggestion",
        "usageStatus": "partial",
    });
    let _ = st.db.insert_usage(&row);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_one_plain_line_and_none_means_nothing() {
        assert_eq!(normalize_answer("\"Run the tests\"").as_deref(), Some("Run the tests"));
        assert_eq!(normalize_answer("NEXT: Add the missing import").as_deref(), Some("Add the missing import"));
        assert_eq!(normalize_answer("\n\n  COULD   you explain why?  \nmore text").as_deref(), Some("COULD you explain why?"), "the first line only, one spacing");
        assert_eq!(normalize_answer("none"), None);
        assert_eq!(normalize_answer("NONE"), None);
        assert_eq!(normalize_answer("No suggestion"), None);
        assert_eq!(normalize_answer("   \n "), None);
        assert_eq!(normalize_answer(&"x".repeat(400)).unwrap().chars().count(), MAX_CHARS, "bounded");
    }

    #[test]
    fn the_prompt_carries_only_the_two_trimmed_parts() {
        let prompt = prompt_for(&"u".repeat(USER_CAP + 10), &"r".repeat(REPLY_CAP + 10));
        assert!(prompt.contains(&"u".repeat(USER_CAP)), "the last message, capped");
        assert!(!prompt.contains(&"u".repeat(USER_CAP + 1)), "and not one character more");
        assert!(prompt.contains(&"r".repeat(REPLY_CAP)));
        assert!(!prompt.contains(&"r".repeat(REPLY_CAP + 1)));
        assert!(prompt.contains("<owner>") && prompt.contains("<reply>"));
    }

    #[test]
    fn clipping_never_splits_a_character() {
        assert_eq!(clip("abcd", 2), "ab");
        assert_eq!(clip("abcd", 9), "abcd");
        assert_eq!(clip("éé", 1), "é", "a two-byte character is cut whole");
    }

    #[test]
    fn usage_is_taken_only_from_what_the_cli_reported() {
        let body = json!({"result":"x","usage":{"input_tokens":12,"output_tokens":3,"cache_read_input_tokens":4,"cache_creation_input_tokens":5}});
        let usage = usage_of(&body, Some("claude-haiku-5-5")).expect("reported numbers are counted");
        assert_eq!(usage["inputTokens"], json!(12));
        assert_eq!(usage["outputTokens"], json!(3));
        assert_eq!(usage["cacheReadTokens"], json!(4));
        assert_eq!(usage["cacheWriteTokens"], json!(5));
        assert_eq!(usage["model"], json!("claude-haiku-5-5"));
        assert!(usage_of(&json!({"result":"x"}), None).is_none(), "no numbers means no row, never a guess");
    }

    #[test]
    fn a_limit_counts_only_while_its_reset_is_ahead() {
        let ahead = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        let past = (chrono::Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        assert!(limit_still_ahead(&json!({"limitedUntil":ahead})));
        assert!(!limit_still_ahead(&json!({"limitedUntil":past})));
        assert!(!limit_still_ahead(&json!({})));
    }
}
