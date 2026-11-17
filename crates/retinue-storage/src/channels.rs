//! Chat channels to the CEO (16 November 2026): one row per channel kind, and
//! the idempotent inbound ledger.
//!
//! Tokens and QR session material never live here. A channel's credentials are
//! in the Windows credential store under `Retinue/channel/<kind>/<name>`, and
//! the WhatsApp Web session folder is under `<data>/channels/`; this file keeps
//! only what the Settings page shows: status, account, paired sender, whether
//! approval buttons are on, and which conversation the channel talks to.

use crate::StorageError;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

/// The three kinds, in the order `channel.list` returns them.
pub const CHANNEL_KINDS: [&str; 3] = ["telegram", "whatsapp-web", "whatsapp-cloud"];

pub fn valid_channel_kind(kind: &str) -> bool {
    CHANNEL_KINDS.contains(&kind)
}

pub fn ensure_channel_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS channels (
            kind TEXT PRIMARY KEY NOT NULL CHECK(kind IN ('telegram','whatsapp-web','whatsapp-cloud')),
            status TEXT NOT NULL DEFAULT 'off' CHECK(status IN ('off','connecting','pairing','connected','error')),
            detail TEXT,
            account TEXT,
            peer_json TEXT CHECK(peer_json IS NULL OR json_valid(peer_json)),
            approvals INTEGER NOT NULL DEFAULT 1 CHECK(approvals IN (0,1)),
            session_id TEXT,
            candidate_json TEXT CHECK(candidate_json IS NULL OR json_valid(candidate_json)),
            last_message_at TEXT,
            updated_at TEXT NOT NULL
        );
        -- One row per inbound message the provider delivered, keyed by the
        -- provider's own id: a retried delivery or a reconnect cannot start the
        -- same turn twice.
        CREATE TABLE IF NOT EXISTS channel_inbox (
            source_id TEXT PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL,
            message_id TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('pending','submitted','discarded','blocked')),
            session_id TEXT,
            turn_id TEXT,
            at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_channel_inbox_kind_at ON channel_inbox(kind, at);",
    )?;
    Ok(())
}

fn row_to_value(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let peer: Option<String> = row.get("peer_json")?;
    let candidate: Option<String> = row.get("candidate_json")?;
    let approvals: i64 = row.get("approvals")?;
    Ok(json!({
        "kind": row.get::<_, String>("kind")?,
        "status": row.get::<_, String>("status")?,
        "detail": row.get::<_, Option<String>>("detail")?,
        "account": row.get::<_, Option<String>>("account")?,
        "peer": peer.and_then(|text| serde_json::from_str::<Value>(&text).ok()),
        "approvals": approvals != 0,
        "sessionId": row.get::<_, Option<String>>("session_id")?,
        "candidate": candidate.and_then(|text| serde_json::from_str::<Value>(&text).ok()),
        "lastMessageAt": row.get::<_, Option<String>>("last_message_at")?,
        "updatedAt": row.get::<_, String>("updated_at")?,
    }))
}

impl crate::Storage {
    /// One channel's stored row, or `None` while it has never been set up.
    pub fn channel_get(&self, kind: &str) -> Result<Option<Value>, StorageError> {
        if !valid_channel_kind(kind) { return Err(StorageError::InvalidAgent); }
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut statement = c.prepare("SELECT kind,status,detail,account,peer_json,approvals,session_id,candidate_json,last_message_at,updated_at FROM channels WHERE kind=?1")?;
        let mut rows = statement.query_map([kind], row_to_value)?;
        Ok(rows.next().transpose()?)
    }

    /// Every stored channel row, oldest kind first in `CHANNEL_KINDS` order.
    pub fn channel_list(&self) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut statement = c.prepare("SELECT kind,status,detail,account,peer_json,approvals,session_id,candidate_json,last_message_at,updated_at FROM channels")?;
        let rows = statement.query_map([], row_to_value)?.collect::<Result<Vec<_>, _>>()?;
        let mut sorted = Vec::with_capacity(rows.len());
        for kind in CHANNEL_KINDS {
            if let Some(row) = rows.iter().find(|row| row["kind"] == kind) { sorted.push(row.clone()); }
        }
        Ok(sorted)
    }

    /// Writes a channel row whole. `row` carries the fields `channel_get`
    /// returns; a missing or null field is stored as NULL, and `approvals`
    /// defaults to on.
    pub fn channel_put(&self, kind: &str, row: &Value) -> Result<(), StorageError> {
        if !valid_channel_kind(kind) { return Err(StorageError::InvalidAgent); }
        let status = row["status"].as_str().unwrap_or("off");
        if !matches!(status, "off" | "connecting" | "pairing" | "connected" | "error") { return Err(StorageError::InvalidAgent); }
        let peer = match &row["peer"] {
            Value::Null => None,
            value @ Value::Object(_) => Some(value.to_string()),
            _ => return Err(StorageError::InvalidAgent),
        };
        let candidate = match &row["candidate"] {
            Value::Null => None,
            value @ Value::Object(_) => Some(value.to_string()),
            _ => return Err(StorageError::InvalidAgent),
        };
        let updated_at = row["updatedAt"].as_str().filter(|at| !at.is_empty()).ok_or(StorageError::InvalidAgent)?;
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO channels(kind,status,detail,account,peer_json,approvals,session_id,candidate_json,last_message_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(kind) DO UPDATE SET status=excluded.status,detail=excluded.detail,account=excluded.account,peer_json=excluded.peer_json,approvals=excluded.approvals,session_id=excluded.session_id,candidate_json=excluded.candidate_json,last_message_at=excluded.last_message_at,updated_at=excluded.updated_at",
            params![
                kind, status,
                row["detail"].as_str(),
                row["account"].as_str(),
                peer,
                if row["approvals"].as_bool().unwrap_or(true) { 1 } else { 0 },
                row["sessionId"].as_str(),
                candidate,
                row["lastMessageAt"].as_str(),
                updated_at,
            ],
        )?;
        Ok(())
    }

    /// Forgets a channel: the token and the paired sender go with it, so the
    /// next connect starts from nothing.
    pub fn channel_delete(&self, kind: &str) -> Result<(), StorageError> {
        if !valid_channel_kind(kind) { return Err(StorageError::InvalidAgent); }
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute("DELETE FROM channels WHERE kind=?1", [kind])?;
        Ok(())
    }

    /// Claims an inbound message id. `true` means this call is the first to see
    /// it and may start work; `false` means it was already handled, so the
    /// provider's retry is dropped.
    pub fn channel_inbox_claim(&self, source_id: &str, kind: &str, message_id: &str, at: &str) -> Result<bool, StorageError> {
        if !valid_channel_kind(kind) { return Err(StorageError::InvalidAgent); }
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let changed = c.execute(
            "INSERT OR IGNORE INTO channel_inbox(source_id,kind,message_id,state,at) VALUES(?1,?2,?3,'pending',?4)",
            params![source_id, kind, message_id, at],
        )?;
        Ok(changed > 0)
    }

    /// Moves a claimed message to its outcome. `submitted` carries the
    /// conversation and turn it started.
    pub fn channel_inbox_finish(&self, source_id: &str, state: &str, session_id: Option<&str>, turn_id: Option<&str>) -> Result<(), StorageError> {
        if !matches!(state, "submitted" | "discarded" | "blocked") { return Err(StorageError::InvalidAgent); }
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "UPDATE channel_inbox SET state=?2,session_id=COALESCE(?3,session_id),turn_id=COALESCE(?4,turn_id) WHERE source_id=?1",
            params![source_id, state, session_id, turn_id],
        )?;
        Ok(())
    }

    /// One ledger row, for the daemon's own bookkeeping and its tests.
    pub fn channel_inbox_row(&self, source_id: &str) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let raw: Option<String> = c
            .query_row(
                "SELECT json_object('sourceId',source_id,'kind',kind,'messageId',message_id,'state',state,'sessionId',session_id,'turnId',turn_id,'at',at) FROM channel_inbox WHERE source_id=?1",
                [source_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|text| serde_json::from_str::<Value>(&text).ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Storage;

    fn db() -> Storage { Storage::open_in_memory().unwrap() }

    #[test]
    fn a_channel_row_round_trips_and_its_unknown_kind_is_refused() {
        let db = db();
        assert_eq!(db.channel_get("telegram").unwrap(), None, "never set up is not a row");
        assert!(db.channel_get("slack").is_err(), "there is no fourth kind");
        db.channel_put("telegram", &json!({
            "status": "connected", "detail": null, "account": "@retinue_bot",
            "peer": {"id": "555", "name": "Vex", "handle": "@vex"},
            "approvals": false, "sessionId": "conversation-1", "candidate": null,
            "lastMessageAt": "2026-11-16T10:00:00.000Z", "updatedAt": "2026-11-16T10:00:00.000Z"
        })).unwrap();
        let row = db.channel_get("telegram").unwrap().unwrap();
        assert_eq!(row["status"], "connected");
        // The numeric id stays in the store; the page reads around it.
        assert_eq!(row["peer"]["id"], "555");
        assert_eq!(row["approvals"], false);
        assert_eq!(db.channel_list().unwrap().len(), 1, "only the kinds that exist are listed");
        // A write replaces the whole row.
        db.channel_put("telegram", &json!({"status": "off", "approvals": true, "updatedAt": "2026-11-16T11:00:00.000Z"})).unwrap();
        let row = db.channel_get("telegram").unwrap().unwrap();
        assert_eq!((row["status"].as_str(), row["peer"].is_null(), row["account"].is_null()), (Some("off"), true, true));
        assert_eq!(db.channel_put("telegram", &json!({"status": "sleeping", "updatedAt": "now"})).unwrap_err().to_string(), "invalid agent", "an unknown status never reaches the table");
        db.channel_delete("telegram").unwrap();
        assert_eq!(db.channel_get("telegram").unwrap(), None);
    }

    #[test]
    fn an_inbound_message_id_is_claimed_once_and_records_its_turn() {
        let db = db();
        let at = "2026-11-16T10:00:00.000Z";
        assert!(db.channel_inbox_claim("channel:telegram:bot:42", "telegram", "42", at).unwrap(), "the first delivery is the one that works");
        assert!(!db.channel_inbox_claim("channel:telegram:bot:42", "telegram", "42", at).unwrap(), "a provider retry claims nothing");
        assert!(db.channel_inbox_claim("channel:telegram:bot:43", "telegram", "43", at).unwrap());
        db.channel_inbox_finish("channel:telegram:bot:42", "submitted", Some("conversation-1"), Some("turn-1")).unwrap();
        let row = db.channel_inbox_row("channel:telegram:bot:42").unwrap().unwrap();
        assert_eq!((row["state"].as_str(), row["sessionId"].as_str(), row["turnId"].as_str()), (Some("submitted"), Some("conversation-1"), Some("turn-1")));
        // A blocked message records no turn, and an unknown state is refused.
        db.channel_inbox_finish("channel:telegram:bot:43", "blocked", None, None).unwrap();
        assert_eq!(db.channel_inbox_row("channel:telegram:bot:43").unwrap().unwrap()["state"], "blocked");
        assert!(db.channel_inbox_finish("channel:telegram:bot:43", "forgotten", None, None).is_err());
    }
}
