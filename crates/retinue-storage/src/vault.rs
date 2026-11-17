//! The vault's metadata (16 November 2026).
//!
//! The vault keeps logins, cards, authenticator keys, API keys and notes. The
//! *values* never reach this database: they live in Windows Credential Manager,
//! one credential per field (`Retinue/vault/<itemId>/<field>`), written and read
//! by the daemon's vault module. This file holds only what the owner may see in a
//! list: the title, the kind, a login's origin, a card's brand and last four.
//!
//! `vault_activity` is the content-free ledger: one row per fill, reveal or
//! change, naming the item, where it happened and who asked. It never holds a
//! value, a typed text, a page's text or a selector, and it is not part of a
//! backup (`backup.rs` writes a curated document, not a database dump).
//!
//! Timestamps are RFC 3339 UTC strings, as the contract spells them out.

use super::{params, Connection, Storage, StorageError, Utc};
use chrono::SecondsFormat;
use rusqlite::Row;
use serde_json::{json, Value};

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_vault_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault_items(
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL CHECK(kind IN ('login','card','apiKey','note')),
            title TEXT NOT NULL,
            origin TEXT,
            username TEXT,
            holder TEXT,
            brand TEXT,
            last4 TEXT,
            expiry TEXT,
            service TEXT,
            has_totp INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_used_at TEXT,
            use_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_vault_items_updated ON vault_items(updated_at DESC);
        CREATE TABLE IF NOT EXISTS vault_activity(
            id TEXT PRIMARY KEY,
            at TEXT NOT NULL,
            item_id TEXT,
            item_title TEXT,
            action TEXT NOT NULL,
            where_ TEXT NOT NULL,
            origin TEXT,
            bot TEXT,
            session_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_vault_activity_at ON vault_activity(at DESC);
        CREATE INDEX IF NOT EXISTS idx_vault_activity_item ON vault_activity(item_id, at DESC);",
    )?;
    Ok(())
}

const ITEM_COLUMNS: &str =
    "id,kind,title,origin,username,holder,brand,last4,expiry,service,has_totp,created_at,updated_at,last_used_at,use_count";

fn item_from_row(r: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, String>("id")?,
        "kind": r.get::<_, String>("kind")?,
        "title": r.get::<_, String>("title")?,
        "origin": r.get::<_, Option<String>>("origin")?,
        "username": r.get::<_, Option<String>>("username")?,
        "holder": r.get::<_, Option<String>>("holder")?,
        "brand": r.get::<_, Option<String>>("brand")?,
        "last4": r.get::<_, Option<String>>("last4")?,
        "expiry": r.get::<_, Option<String>>("expiry")?,
        "service": r.get::<_, Option<String>>("service")?,
        "hasTotp": r.get::<_, i64>("has_totp")? != 0,
        "createdAt": r.get::<_, String>("created_at")?,
        "updatedAt": r.get::<_, String>("updated_at")?,
        "lastUsedAt": r.get::<_, Option<String>>("last_used_at")?,
        "useCount": r.get::<_, i64>("use_count")?,
    }))
}

fn activity_from_row(r: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": r.get::<_, String>("id")?,
        "at": r.get::<_, String>("at")?,
        "itemId": r.get::<_, Option<String>>("item_id")?,
        "itemTitle": r.get::<_, Option<String>>("item_title")?,
        "action": r.get::<_, String>("action")?,
        "where": r.get::<_, String>("where_")?,
        "origin": r.get::<_, Option<String>>("origin")?,
        "bot": r.get::<_, Option<String>>("bot")?,
        "sessionId": r.get::<_, Option<String>>("session_id")?,
    }))
}

/// One column of an item as text: `None` for a missing key or a JSON null.
fn text(item: &Value, key: &str) -> Option<String> {
    item.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn required(item: &Value, key: &str) -> Result<String, StorageError> {
    text(item, key).ok_or(StorageError::InvalidAgent)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl Storage {
    /// Every item, newest change first. Metadata only: no value is stored here.
    pub fn vault_items(&self) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare(&format!("SELECT {ITEM_COLUMNS} FROM vault_items ORDER BY updated_at DESC, id"))?;
        let rows = q.query_map([], item_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// One item, or `None` when it is gone.
    pub fn vault_item(&self, id: &str) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare(&format!("SELECT {ITEM_COLUMNS} FROM vault_items WHERE id=?1"))?;
        let mut rows = q.query_map([id], item_from_row)?;
        Ok(rows.next().transpose()?)
    }

    /// Inserts a new item. `item` carries `id`, `kind`, `title` and the kind's
    /// optional columns; `createdAt` and `updatedAt` are filled here.
    pub fn vault_item_insert(&self, item: &Value) -> Result<Value, StorageError> {
        let id = required(item, "id")?;
        let kind = required(item, "kind")?;
        let title = required(item, "title")?;
        let at = now();
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO vault_items(id,kind,title,origin,username,holder,brand,last4,expiry,service,has_totp,created_at,updated_at,last_used_at,use_count)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,NULL,0)",
            params![
                id,
                kind,
                title,
                text(item, "origin"),
                text(item, "username"),
                text(item, "holder"),
                text(item, "brand"),
                text(item, "last4"),
                text(item, "expiry"),
                text(item, "service"),
                i64::from(item["hasTotp"] == true),
                at,
            ],
        )?;
        // Release the connection first: `vault_item` takes the same lock, and a
        // std Mutex held twice on one thread never returns.
        drop(c);
        self.vault_item(&id)?.ok_or(StorageError::InvalidAgent)
    }

    /// Replaces one item's metadata (never its values) and moves `updatedAt`.
    /// `false` when the item no longer exists.
    pub fn vault_item_update(&self, id: &str, item: &Value) -> Result<bool, StorageError> {
        let kind = required(item, "kind")?;
        let title = required(item, "title")?;
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let changed = c.execute(
            "UPDATE vault_items SET kind=?2,title=?3,origin=?4,username=?5,holder=?6,brand=?7,last4=?8,expiry=?9,service=?10,has_totp=?11,updated_at=?12 WHERE id=?1",
            params![
                id,
                kind,
                title,
                text(item, "origin"),
                text(item, "username"),
                text(item, "holder"),
                text(item, "brand"),
                text(item, "last4"),
                text(item, "expiry"),
                text(item, "service"),
                i64::from(item["hasTotp"] == true),
                now(),
            ],
        )?;
        Ok(changed > 0)
    }

    /// Counts one use and records when, so the list can say "used 3 times, last
    /// on 12 Nov". `false` when the item no longer exists.
    pub fn vault_item_touch(&self, id: &str, at: &str) -> Result<bool, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let changed = c.execute("UPDATE vault_items SET use_count=use_count+1,last_used_at=?2 WHERE id=?1", params![id, at])?;
        Ok(changed > 0)
    }

    /// Removes one item's metadata row. The caller removes the credentials.
    pub fn vault_item_delete(&self, id: &str) -> Result<bool, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let removed = c.execute("DELETE FROM vault_items WHERE id=?1", [id])?;
        Ok(removed > 0)
    }

    /// One content-free ledger row.
    pub fn vault_activity_add(&self, row: &Value) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO vault_activity(id,at,item_id,item_title,action,where_,origin,bot,session_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                required(row, "id")?,
                text(row, "at").unwrap_or_else(now),
                text(row, "itemId"),
                text(row, "itemTitle"),
                required(row, "action")?,
                required(row, "where")?,
                text(row, "origin"),
                text(row, "bot"),
                text(row, "sessionId"),
            ],
        )?;
        Ok(())
    }

    /// The ledger, newest first, for one item or for everything, capped.
    pub fn vault_activity(&self, item_id: Option<&str>, limit: u32) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare(
            "SELECT id,at,item_id,item_title,action,where_,origin,bot,session_id FROM vault_activity
             WHERE (?1 IS NULL OR item_id=?1) ORDER BY at DESC, id DESC LIMIT ?2",
        )?;
        let rows = q.query_map(params![item_id, i64::from(limit)], activity_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
