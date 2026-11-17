//! Connectors: owner-added MCP servers the daemon calls (plan `nod-adoption-plan.md`, section D).
//!
//! A row holds the name, address, auth kind, switches and the last check. The
//! token never reaches this database: it lives in Credential Manager under
//! `Retinue/connector/<id>`. The tool schemas from the last successful check are
//! cached as JSON so search and the settings page work without a network call;
//! the per-tool approval rules are a second table, so removing a connector
//! removes both.

use super::{params, OptionalExtension, SecondsFormat, Storage, StorageError, Utc, Connection};
use serde_json::Value;
use std::collections::BTreeMap;

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_connectors_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS connectors(
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        url TEXT NOT NULL,
        auth TEXT NOT NULL DEFAULT 'none' CHECK(auth IN ('none','token','oauth')),
        enabled INTEGER NOT NULL DEFAULT 1,
        unattended TEXT NOT NULL DEFAULT 'off' CHECK(unattended IN ('off','readOnly')),
        status TEXT NOT NULL DEFAULT 'unchecked' CHECK(status IN ('ok','error','unchecked')),
        detail TEXT,
        tool_count INTEGER,
        tools_json TEXT NOT NULL DEFAULT '[]',
        last_checked_at TEXT,
        created_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS connector_tool_rules(
        connector_id TEXT NOT NULL,
        tool TEXT NOT NULL,
        rule TEXT NOT NULL CHECK(rule IN ('ask','allow','deny')),
        updated_at TEXT NOT NULL,
        PRIMARY KEY (connector_id, tool)
    );",
    )?;
    Ok(())
}

const CONNECTOR_SQL: &str = "SELECT id,name,url,auth,enabled,unattended,status,detail,tool_count,tools_json,last_checked_at FROM connectors";

type ConnectorRow = (String, String, String, String, i64, String, String, Option<String>, Option<i64>, String, Option<String>);

fn connector_dto(row: ConnectorRow) -> Value {
    let (id, name, url, auth, enabled, unattended, status, detail, tool_count, _tools, last_checked) = row;
    serde_json::json!({
        "id": id,
        "name": name,
        "url": url,
        "auth": auth,
        "enabled": enabled != 0,
        "status": status,
        "detail": detail,
        "toolCount": tool_count,
        "unattended": unattended,
        "lastCheckedAt": last_checked,
    })
}

fn connector_columns(r: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectorRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl Storage {
    pub fn connector_list(&self) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare(&format!("{CONNECTOR_SQL} ORDER BY name COLLATE NOCASE, id"))?;
        let rows = q.query_map([], connector_columns)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().map(connector_dto).collect())
    }

    pub fn connector_get(&self, id: &str) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let row = c.query_row(&format!("{CONNECTOR_SQL} WHERE id=?1"), [id], connector_columns).optional()?;
        Ok(row.map(connector_dto))
    }

    pub fn connector_insert(&self, id: &str, name: &str, url: &str, auth: &str, unattended: &str) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO connectors(id,name,url,auth,enabled,unattended,status,detail,tool_count,tools_json,last_checked_at,created_at) VALUES(?1,?2,?3,?4,1,?5,'unchecked',NULL,NULL,'[]',NULL,?6)",
            params![id, name, url, auth, unattended, now()],
        )?;
        Ok(())
    }

    /// Only the fields a caller names change; `None` leaves one as it was.
    pub fn connector_update(&self, id: &str, name: Option<&str>, auth: Option<&str>, enabled: Option<bool>, unattended: Option<&str>) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let enabled = enabled.map(|value| if value { 1i64 } else { 0 });
        let changed = c.execute(
            "UPDATE connectors SET name=COALESCE(?2,name), auth=COALESCE(?3,auth), enabled=COALESCE(?4,enabled), unattended=COALESCE(?5,unattended) WHERE id=?1",
            params![id, name, auth, enabled, unattended],
        )?;
        if changed == 0 { return Ok(None); }
        let row = c.query_row(&format!("{CONNECTOR_SQL} WHERE id=?1"), [id], connector_columns).optional()?;
        Ok(row.map(connector_dto))
    }

    /// What a check found: the status, a plain sentence, the tool count and the
    /// schemas search reads. Nothing here holds a token.
    pub fn connector_set_check(&self, id: &str, status: &str, detail: Option<&str>, tool_count: Option<i64>, tools: &Value) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "UPDATE connectors SET status=?2, detail=?3, tool_count=?4, tools_json=?5, last_checked_at=?6 WHERE id=?1",
            params![id, status, detail, tool_count, tools.to_string(), now()],
        )?;
        Ok(())
    }

    /// A later call changed the status without a fresh tool list (a transport
    /// failure, say). The cached schemas stay.
    pub fn connector_set_status(&self, id: &str, status: &str, detail: Option<&str>, tool_count: Option<i64>) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute("UPDATE connectors SET status=?2, detail=?3, tool_count=?4 WHERE id=?1", params![id, status, detail, tool_count])?;
        Ok(())
    }

    /// The cached tools of a connector, as the server last listed them.
    pub fn connector_tools(&self, id: &str) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let raw: Option<String> = c.query_row("SELECT tools_json FROM connectors WHERE id=?1", [id], |r| r.get(0)).optional()?;
        Ok(raw.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()).and_then(|value| value.as_array().cloned()).unwrap_or_default())
    }

    pub fn connector_delete(&self, id: &str) -> Result<bool, StorageError> {
        let mut c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM connector_tool_rules WHERE connector_id=?1", [id])?;
        let removed = tx.execute("DELETE FROM connectors WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(removed != 0)
    }

    pub fn connector_rule_set(&self, id: &str, tool: &str, rule: &str) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO connector_tool_rules(connector_id,tool,rule,updated_at) VALUES(?1,?2,?3,?4)
             ON CONFLICT(connector_id,tool) DO UPDATE SET rule=excluded.rule, updated_at=excluded.updated_at",
            params![id, tool, rule, now()],
        )?;
        Ok(())
    }

    /// Each tool's standing rule; a tool with no row is `ask`.
    pub fn connector_rules(&self, id: &str) -> Result<BTreeMap<String, String>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare("SELECT tool,rule FROM connector_tool_rules WHERE connector_id=?1")?;
        let rows = q.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows.into_iter().collect())
    }
}
