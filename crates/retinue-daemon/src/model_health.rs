//! Wave 2 (nod adoption, 17 November 2026): a per-model failure ledger that
//! pauses only unattended work.
//!
//! Retinue already notices a failed turn and a usage limit, but nothing remembers
//! across restarts that a model failed an hour ago, so every scheduled run in
//! that hour paid for the same failure. nod keeps a small ladder per model
//! (`02-llm-providers.md`, idea 2); this is the same shape, with three promises:
//!
//! 1. **Only unattended work is paused.** An owner's own message always runs: the
//!    owner may know something the daemon does not, and hiding a model from them
//!    would be the app deciding for them.
//! 2. **Nothing is switched.** The conversation is refused with a plain sentence;
//!    the daemon never moves it to another model on its own.
//! 3. **It is not a budget.** No spend is counted or limited here; budgets are
//!    retired. This is only "that model just failed for everyone".
//!
//! A completed turn deletes the row, so one good answer clears it.

use crate::{AppState, DispatchError};
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::Value;

/// How many failures in a row, with no completed turn between them, before
/// unattended work pauses. One failure can be a hiccup; the second is a pattern.
const PAUSE_AFTER: i64 = 2;
/// How long the pause lasts, by how far past the threshold the count is: five
/// minutes, then fifteen, then an hour, and an hour from then on.
const PAUSE_STEPS_SECS: [i64; 3] = [300, 900, 3_600];

/// Whether a failure class is the model's own trouble, rather than ours, the
/// owner's or something a retry cannot fix. Cancellations, a full context window
/// and a refused permission never pause anything.
fn counts(failure_class: &str) -> bool {
    matches!(failure_class, "provider_error" | "provider_unavailable" | "overloaded" | "unauthenticated" | "config_unsupported" | "auth")
}

/// The pause a failure count earns, in seconds, or `None` below the threshold.
fn pause_secs(failures: i64) -> Option<i64> {
    if failures < PAUSE_AFTER { return None; }
    let step = usize::try_from(failures - PAUSE_AFTER).unwrap_or(usize::MAX).min(PAUSE_STEPS_SECS.len() - 1);
    Some(PAUSE_STEPS_SECS[step])
}

/// The provider and model a conversation runs on, when both are known. A
/// conversation with no model lock follows its bot's model, which is what the
/// launcher does.
pub(crate) fn session_key(st: &AppState, session_id: &str) -> Option<(String, String)> {
    let brief = st.db.session_brief(session_id).ok()?;
    let provider = brief["provider"].as_str().map(str::trim).filter(|provider| !provider.is_empty())?.to_owned();
    let model = st.db.session_execution_row(session_id).ok().and_then(|row| {
        row["modelLock"]["model"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| row["agentId"].as_str().and_then(|id| st.db.agent_get(id).ok()).and_then(|agent| agent["model"].as_str().map(str::to_owned)))
    })?;
    Some((provider, model))
}

/// Records one failed turn for the conversation's model. Silent when the
/// conversation's model is unknown, or when the class is not the model's trouble.
pub(crate) fn note_failure(st: &AppState, session_id: &str, failure_class: &str, detail: Option<&str>) {
    if !counts(failure_class) { return; }
    let Some((provider, model)) = session_key(st, session_id) else { return };
    record_failure(st, &provider, &model, detail);
}

/// The same, for a caller that already knows which model it used.
pub(crate) fn record_failure(st: &AppState, provider: &str, model: &str, detail: Option<&str>) {
    // The reason is the provider's own line, redacted exactly as a turn error's
    // detail is, so nothing a credential marker hides can reach the ledger.
    let reason = detail.and_then(crate::safe_provider_error_detail);
    let next_count = st.db.model_health_get(provider, model).ok().flatten().map(|row| row["failures"].as_i64().unwrap_or(0)).unwrap_or(0) + 1;
    let paused_until = pause_secs(next_count).map(|secs| (Utc::now() + Duration::seconds(secs)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    let _ = st.db.model_health_record(provider, model, reason.as_deref(), paused_until.as_deref());
}

/// Forgets a model's failures: a completed turn proves it works again.
pub(crate) fn note_success(st: &AppState, session_id: &str) {
    if let Some((provider, model)) = session_key(st, session_id) {
        let _ = st.db.model_health_clear(&provider, &model);
    }
}

/// `Some(error)` when this turn is unattended and its model is paused, so the
/// caller refuses before anything starts. An owner's turn is never refused.
pub(crate) fn refusal(st: &AppState, session_id: &str, link: &Value) -> Option<DispatchError> {
    if !crate::is_unattended(link) { return None; }
    let (provider, model) = session_key(st, session_id)?;
    let row = st.db.model_health_get(&provider, &model).ok().flatten()?;
    let until = row["pausedUntil"].as_str()?;
    let when = chrono::DateTime::parse_from_rfc3339(until).ok().map(|at| at.with_timezone(&Utc).format("%H:%M UTC").to_string()).unwrap_or_else(|| "a little later".to_owned());
    Some(crate::derr(
        "model_paused",
        &format!("This model failed twice in a row, so unattended work on it is paused until {when}. Run the conversation yourself, or choose another model."),
        StatusCode::CONFLICT,
    ))
}

/// The ledger keyed by `provider` and `model`, for the model catalogue's `health`
/// field. Empty when nothing has failed.
pub(crate) fn rows(st: &AppState) -> Vec<Value> {
    st.db.model_health_all().unwrap_or_default()
}

/// The `health` value for one catalogue row, or `Value::Null` when the ledger has
/// nothing about it.
pub(crate) fn health_for(rows: &[Value], provider: &str, model: &str) -> Value {
    rows.iter()
        .find(|row| row["provider"].as_str() == Some(provider) && row["model"].as_str() == Some(model))
        .map(|row| {
            serde_json::json!({
                "failures": row["failures"],
                "pausedUntil": row["pausedUntil"],
                "reason": row["reason"],
            })
        })
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_failure_pauses_nothing_and_the_pause_grows_after_the_second() {
        assert_eq!(pause_secs(0), None);
        assert_eq!(pause_secs(1), None);
        assert_eq!(pause_secs(2), Some(300));
        assert_eq!(pause_secs(3), Some(900));
        assert_eq!(pause_secs(4), Some(3_600));
        assert_eq!(pause_secs(90), Some(3_600), "the ladder stops at an hour");
    }

    #[test]
    fn only_a_models_own_trouble_counts() {
        for class in ["provider_error", "provider_unavailable", "unauthenticated", "config_unsupported"] {
            assert!(counts(class), "{class} is the model's trouble");
        }
        for class in ["timeout", "context", "permission_denied", "cancelled"] {
            assert!(!counts(class), "{class} is not, so it pauses nothing");
        }
    }

    #[test]
    fn the_health_field_is_shaped_for_the_catalogue() {
        let rows = vec![serde_json::json!({"provider":"codex","model":"gpt-6-luna","failures":2,"pausedUntil":"2026-11-17T12:30:00.000Z","reason":"The provider reported a turn error.","paused":true})];
        let health = health_for(&rows, "codex", "gpt-6-luna");
        assert_eq!(health["failures"], 2);
        assert_eq!(health["pausedUntil"], "2026-11-17T12:30:00.000Z");
        assert_eq!(health["reason"], "The provider reported a turn error.");
        assert!(health_for(&rows, "claude", "claude-sonnet-5-5").is_null());
    }
}
