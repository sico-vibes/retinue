//! Wave 2 storage (nod adoption, 17 November 2026): a failure ledger per model.
//!
//! One row per provider and model, so the daemon can remember across restarts that
//! a model failed an hour ago. `02-llm-providers.md`, idea 2: the ledger pauses
//! **unattended** work only — an owner's own turn always runs, and nothing is ever
//! switched or hidden. A completed turn deletes the row.
//!
//! The row holds no message text and no credential: only the provider, the model,
//! a count, two timestamps and the provider's own line, which the daemon has
//! already redacted before it gets here (`safe_provider_error_detail`).

use super::{params, Connection, OptionalExtension, Storage, StorageError};
use serde_json::{json, Value};

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_wave2_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS model_health(
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            failures INTEGER NOT NULL DEFAULT 0,
            last_failure_at TEXT NOT NULL,
            paused_until TEXT,
            reason TEXT,
            PRIMARY KEY(provider, model)
        );",
    )?;
    Ok(())
}

/// One row as the daemon and the window read it. `paused` is computed against
/// `now`, so a caller never has to compare timestamps itself.
fn row_value(row: (String, String, i64, String, Option<String>, Option<String>), now: &str) -> Value {
    let (provider, model, failures, last_failure_at, paused_until, reason) = row;
    let paused = paused_until.as_deref().is_some_and(|until| until > now);
    json!({
        "provider": provider,
        "model": model,
        "failures": failures,
        "lastFailureAt": last_failure_at,
        "pausedUntil": paused_until.filter(|_| paused),
        "reason": reason,
        "paused": paused,
    })
}

impl Storage {
    /// Every recorded failure, newest first. Empty in the ordinary case.
    pub fn model_health_all(&self) -> Result<Vec<Value>, StorageError> {
        let now = super::Utc::now().to_rfc3339_opts(super::SecondsFormat::Millis, true);
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut query = c.prepare("SELECT provider, model, failures, last_failure_at, paused_until, reason FROM model_health ORDER BY last_failure_at DESC")?;
        let rows = query
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(|row| row_value(row, &now)).collect())
    }

    /// The row for one model, if anything has been recorded for it.
    pub fn model_health_get(&self, provider: &str, model: &str) -> Result<Option<Value>, StorageError> {
        let now = super::Utc::now().to_rfc3339_opts(super::SecondsFormat::Millis, true);
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let row = c
            .query_row(
                "SELECT provider, model, failures, last_failure_at, paused_until, reason FROM model_health WHERE provider=?1 AND model=?2",
                params![provider, model],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;
        Ok(row.map(|row| row_value(row, &now)))
    }

    /// Records one failure: the count goes up, the reason is the newest one, and
    /// `paused_until` is what the caller worked out from the count.
    pub fn model_health_record(&self, provider: &str, model: &str, reason: Option<&str>, paused_until: Option<&str>) -> Result<i64, StorageError> {
        let now = super::Utc::now().to_rfc3339_opts(super::SecondsFormat::Millis, true);
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO model_health(provider, model, failures, last_failure_at, paused_until, reason)
             VALUES(?1, ?2, 1, ?3, ?4, ?5)
             ON CONFLICT(provider, model) DO UPDATE SET
                failures = MIN(model_health.failures + 1, 999),
                last_failure_at = excluded.last_failure_at,
                paused_until = excluded.paused_until,
                reason = excluded.reason",
            params![provider, model, now, paused_until, reason],
        )?;
        let failures: i64 = c.query_row("SELECT failures FROM model_health WHERE provider=?1 AND model=?2", params![provider, model], |r| r.get(0))?;
        Ok(failures)
    }

    /// Forgets a model's failures: a completed turn proves it works again.
    /// Returns `true` when there was something to forget.
    pub fn model_health_clear(&self, provider: &str, model: &str) -> Result<bool, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let removed = c.execute("DELETE FROM model_health WHERE provider=?1 AND model=?2", params![provider, model])?;
        Ok(removed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A queue and a ledger on an in-memory database: nothing here touches a file.
    fn store() -> Storage { Storage::open_in_memory().unwrap() }

    #[test]
    fn an_empty_ledger_answers_nothing_for_any_model() {
        let db = store();
        assert!(db.model_health_all().unwrap().is_empty());
        assert!(db.model_health_get("codex", "gpt-6-luna").unwrap().is_none());
    }

    #[test]
    fn two_failures_count_up_and_the_pause_is_what_was_asked_for() {
        let db = store();
        assert_eq!(db.model_health_record("codex", "gpt-6-luna", Some("The provider reported a turn error."), None).unwrap(), 1);
        assert_eq!(db.model_health_record("codex", "gpt-6-luna", Some("The provider reported a turn error."), Some("2999-01-01T00:00:00.000Z")).unwrap(), 2);
        let row = db.model_health_get("codex", "gpt-6-luna").unwrap().expect("a row");
        assert_eq!(row["failures"], 2);
        assert_eq!(row["reason"], "The provider reported a turn error.");
        assert!(row["paused"].as_bool().unwrap(), "a pause in the future is in force");
        assert_eq!(row["pausedUntil"], "2999-01-01T00:00:00.000Z");
        assert_eq!(db.model_health_all().unwrap().len(), 1);
    }

    #[test]
    fn a_pause_that_has_run_out_is_no_longer_in_force_but_the_failure_is_remembered() {
        let db = store();
        db.model_health_record("claude", "claude-sonnet-5-5", None, Some("2000-01-01T00:00:00.000Z")).unwrap();
        let row = db.model_health_get("claude", "claude-sonnet-5-5").unwrap().expect("a row");
        assert!(!row["paused"].as_bool().unwrap());
        assert!(row["pausedUntil"].is_null(), "an expired pause is not reported as one");
        assert_eq!(row["failures"], 1);
    }

    #[test]
    fn a_good_turn_forgets_the_model_and_one_provider_never_hides_another() {
        let db = store();
        db.model_health_record("codex", "gpt-6-luna", None, None).unwrap();
        db.model_health_record("claude", "claude-sonnet-5-5", None, None).unwrap();
        assert!(db.model_health_clear("codex", "gpt-6-luna").unwrap());
        assert!(db.model_health_get("codex", "gpt-6-luna").unwrap().is_none());
        assert!(db.model_health_get("claude", "claude-sonnet-5-5").unwrap().is_some());
        assert!(!db.model_health_clear("codex", "gpt-6-luna").unwrap(), "nothing left to forget");
    }
}
