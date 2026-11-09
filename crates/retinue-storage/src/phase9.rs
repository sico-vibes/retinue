//! Phase 9 storage: employees, hiring and the CEO.
//!
//! Bots became employees (owner decision, 8 November 2026). A bot can be hired
//! into many projects, so `project_members` is the truth about who works where;
//! `agents.project_id` stays only as the bot's default project. A preset
//! employee carries the catalogue id it was made from in `agents.preset_id`, so
//! hiring the same preset again reuses that employee instead of making another.
//!
//! The one-time backfill seeds a membership for every bot that already works in
//! a project, so an existing install opens with the right teams. The CEO
//! (`system_role='manager'`) belongs to every project implicitly and gets no
//! rows.
//!
//! `hired_at` is Unix milliseconds UTC, as the contract spells it out.

use super::{
    params, Connection, DateTime, OptionalExtension, SecondsFormat, Storage, StorageError, TransactionBehavior, Utc,
};
use rusqlite::Row;
use serde_json::{json, Value};

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_phase9_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_members(
            project_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            hired_at INTEGER NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(project_id, agent_id)
        );
        CREATE INDEX IF NOT EXISTS idx_project_members_agent ON project_members(agent_id);",
    )?;
    let has: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('agents') WHERE name='preset_id')",
        [],
        |r| r.get(0),
    )?;
    if !has {
        c.execute_batch("ALTER TABLE agents ADD COLUMN preset_id TEXT")?;
    }
    c.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_preset ON agents(preset_id) WHERE preset_id IS NOT NULL;")?;
    Ok(())
}

/// The one-time data steps: seed the memberships an existing install already
/// has, then rename the built-in Manager to the CEO. Both keep a settings
/// marker, so running them again changes nothing.
pub(super) fn ensure_phase9_data(c: &Connection) -> Result<(), StorageError> {
    seed_project_members(c)?;
    rename_manager_to_ceo(c)?;
    replace_default_playbook(c)?;
    Ok(())
}

const MEMBERS_MARKER: &str = "migration.projectMembersSeeded";
const CEO_MARKER: &str = "migration.ceoRename";
const PLAYBOOK_MARKER: &str = "migration.ceoPlaybook";

/// The playbook says CEO now. A stored playbook still exactly as it shipped is
/// the untouched default, so the new text is added as a new version; one the
/// owner edited is theirs and is left alone.
fn replace_default_playbook(c: &Connection) -> Result<(), StorageError> {
    let done: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key=?1)", [PLAYBOOK_MARKER], |r| r.get(0))?;
    if done {
        return Ok(());
    }
    let current: Option<String> = c
        .query_row("SELECT text FROM manager_playbook ORDER BY version DESC LIMIT 1", [], |r| r.get(0))
        .optional()?;
    if current.as_deref().map(str::trim) == Some(super::DEFAULT_PLAYBOOK_V1.trim()) {
        let version: i64 = c.query_row("SELECT COALESCE(MAX(version),0)+1 FROM manager_playbook", [], |r| r.get(0))?;
        c.execute(
            "INSERT INTO manager_playbook(version,text,author,created_at) VALUES(?1,?2,'default',?3)",
            params![version, super::DEFAULT_PLAYBOOK.trim(), Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)],
        )?;
    }
    c.execute("INSERT OR IGNORE INTO settings(key,value) VALUES(?1,'true')", [PLAYBOOK_MARKER])?;
    Ok(())
}

/// Every bot that already works in a project becomes a member of it. The
/// earliest conversation in that project is the hire date; a bot whose only tie
/// is its default project is hired now.
fn seed_project_members(c: &Connection) -> Result<(), StorageError> {
    let done: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key=?1)", [MEMBERS_MARKER], |r| r.get(0))?;
    if done {
        return Ok(());
    }
    // The CEO is left out: it belongs to every project implicitly.
    // A conversation's project is its stamp, else the project whose folder it
    // works in — the same rule `agent_conversations` reads.
    let rows: Vec<(String, String, String)> = {
        let mut q = c.prepare(
            "SELECT agent_id, project_id, created_at FROM (
                SELECT s.agent_id AS agent_id, COALESCE(px.id, py.id) AS project_id, s.created_at AS created_at
                  FROM sessions s JOIN agents a ON a.id=s.agent_id
                  LEFT JOIN projects px ON px.id=json_extract(s.data,'$.projectId')
                  LEFT JOIN projects py ON py.path=s.project_path
                 WHERE s.archived=0 AND (a.system_role IS NULL OR a.system_role<>'manager')
                   AND (json_extract(s.data,'$.link.kind') IS NULL OR json_extract(s.data,'$.link.kind') NOT IN ('automation','handoff'))
                UNION ALL
                SELECT json_extract(s.data,'$.link.peerAgentId') AS agent_id, COALESCE(px.id, py.id) AS project_id, s.created_at AS created_at
                  FROM sessions s JOIN agents a ON a.id=json_extract(s.data,'$.link.peerAgentId')
                  LEFT JOIN projects px ON px.id=json_extract(s.data,'$.projectId')
                  LEFT JOIN projects py ON py.path=s.project_path
                 WHERE s.archived=0 AND (a.system_role IS NULL OR a.system_role<>'manager')
                   AND json_extract(s.data,'$.link.kind')='side'
            ) WHERE agent_id IS NOT NULL AND project_id IS NOT NULL",
        )?;
        let iter = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        iter.collect::<Result<Vec<_>, _>>()?
    };
    let mut hires: Vec<(String, String, i64)> = Vec::new();
    for (agent_id, project_id, created_at) in rows {
        let at = DateTime::parse_from_rfc3339(&created_at).map(|at| at.timestamp_millis()).unwrap_or_else(|_| Utc::now().timestamp_millis());
        match hires.iter_mut().find(|hire| hire.0 == agent_id && hire.1 == project_id) {
            Some(hire) => hire.2 = hire.2.min(at),
            None => hires.push((agent_id, project_id, at)),
        }
    }
    let defaults: Vec<(String, String)> = {
        let mut q = c.prepare("SELECT id, project_id FROM agents WHERE project_id IS NOT NULL AND (system_role IS NULL OR system_role<>'manager')")?;
        let iter = q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        iter.collect::<Result<Vec<_>, _>>()?
    };
    let now = Utc::now().timestamp_millis();
    let tx = c.unchecked_transaction()?;
    for (agent_id, project_id) in defaults {
        if !hires.iter().any(|hire| hire.0 == agent_id && hire.1 == project_id) {
            hires.push((agent_id, project_id, now));
        }
    }
    for (agent_id, project_id, hired_at) in hires {
        tx.execute(
            "INSERT OR IGNORE INTO project_members(project_id,agent_id,hired_at,sort_order) VALUES(?1,?2,?3,COALESCE((SELECT MAX(sort_order)+1 FROM project_members WHERE project_id=?1),0))",
            params![project_id, agent_id, hired_at],
        )?;
    }
    tx.execute("INSERT OR IGNORE INTO settings(key,value) VALUES(?1,'true')", [MEMBERS_MARKER])?;
    tx.commit()?;
    Ok(())
}

/// The Manager is the CEO now (owner decision, 8 November 2026). Only the
/// default name and role are rewritten: a name the owner chose stays. A bot the
/// owner already named CEO blocks the rename, because two active bots cannot
/// share a name.
fn rename_manager_to_ceo(c: &Connection) -> Result<(), StorageError> {
    let done: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM settings WHERE key=?1)", [CEO_MARKER], |r| r.get(0))?;
    if done {
        return Ok(());
    }
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    c.execute(
        "UPDATE agents SET name='CEO',name_key='ceo',updated_at=?1 WHERE system_role='manager' AND name='Manager'
           AND NOT EXISTS(SELECT 1 FROM agents other WHERE other.name_key='ceo' AND other.archived=0)",
        [&now],
    )?;
    c.execute(
        "UPDATE agents SET role='CEO',updated_at=?1 WHERE system_role='manager' AND role='Manager'",
        [&now],
    )?;
    c.execute("INSERT OR IGNORE INTO settings(key,value) VALUES(?1,'true')", [CEO_MARKER])?;
    Ok(())
}

/// Whether a bot body built from a preset passes the storage rules, so a bad
/// entry in the catalogue fails a test instead of a hire. `valid_look` holds
/// the portrait allowlist, which is the check that matters most here.
pub fn agent_accepts_preset(fields: &Value) -> bool {
    super::validate_agent_fields(fields, false).is_ok()
        && fields.get("tags").is_none_or(|tags| super::validate_entity_tags(tags).is_ok())
}

fn member_row(agent_id: &str, hired_at: i64, sort_order: i64) -> Value {
    json!({"agentId":agent_id,"hiredAt":hired_at,"sortOrder":sort_order})
}

/// One `project_members` row, in column order `agent_id, hired_at, sort_order`.
fn member_from_row(r: &Row<'_>) -> rusqlite::Result<Value> {
    let agent_id: String = r.get(0)?;
    let hired_at: i64 = r.get(1)?;
    let sort_order: i64 = r.get(2)?;
    Ok(member_row(&agent_id, hired_at, sort_order))
}

impl Storage {
    /// Every member of one project, in the project's own order.
    pub fn project_members(&self, project_id: &str) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare("SELECT agent_id,hired_at,sort_order FROM project_members WHERE project_id=?1 ORDER BY sort_order, hired_at, agent_id")?;
        let rows = q.query_map([project_id], member_from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// One membership, or `None` when the bot is not hired into the project.
    pub fn project_member(&self, project_id: &str, agent_id: &str) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare("SELECT agent_id,hired_at,sort_order FROM project_members WHERE project_id=?1 AND agent_id=?2")?;
        let mut rows = q.query_map(params![project_id, agent_id], member_from_row)?;
        Ok(rows.next().transpose()?)
    }

    /// Hires a bot into a project. Hiring twice changes nothing and returns the
    /// row that was already there; `created` is false then.
    pub fn project_add_member(&self, project_id: &str, agent_id: &str) -> Result<(Value, bool), StorageError> {
        let mut c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let project: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)", [project_id], |r| r.get(0))?;
        if !project {
            return Err(StorageError::AgentNotFound);
        }
        let existing: Option<Value> = {
            let mut q = tx.prepare("SELECT agent_id,hired_at,sort_order FROM project_members WHERE project_id=?1 AND agent_id=?2")?;
            let mut rows = q.query_map(params![project_id, agent_id], member_from_row)?;
            rows.next().transpose()?
        };
        if let Some(member) = existing {
            tx.commit()?;
            return Ok((member, false));
        }
        let now = Utc::now().timestamp_millis();
        let order: i64 = tx.query_row("SELECT COALESCE(MAX(sort_order)+1,0) FROM project_members WHERE project_id=?1", [project_id], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO project_members(project_id,agent_id,hired_at,sort_order) VALUES(?1,?2,?3,?4)",
            params![project_id, agent_id, now, order],
        )?;
        tx.commit()?;
        Ok((member_row(agent_id, now, order), true))
    }

    /// Releases a bot from one project. Its conversations, the bot and its
    /// default project are untouched, except that a default pointing at this
    /// project is cleared (the bot no longer works there).
    pub fn project_release(&self, project_id: &str, agent_id: &str) -> Result<bool, StorageError> {
        let mut c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = tx.execute("DELETE FROM project_members WHERE project_id=?1 AND agent_id=?2", params![project_id, agent_id])?;
        if removed > 0 {
            tx.execute("UPDATE agents SET project_id=NULL WHERE id=?1 AND project_id=?2", params![agent_id, project_id])?;
        }
        tx.commit()?;
        Ok(removed > 0)
    }

    /// Every project the bot is hired into, in the order it was hired.
    pub fn project_ids_for_agent(&self, agent_id: &str) -> Result<Vec<String>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare("SELECT project_id FROM project_members WHERE agent_id=?1 ORDER BY sort_order, hired_at, project_id")?;
        let rows = q.query_map([agent_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The live bot made from each preset: `[{presetId, agentId}]`. One read for
    /// the whole catalogue, so `preset.list` never queries per preset.
    pub fn preset_agents(&self) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut q = c.prepare("SELECT preset_id,id FROM agents WHERE preset_id IS NOT NULL AND archived=0 ORDER BY created_at,id")?;
        let rows = q
            .query_map([], |r| Ok(json!({"presetId": r.get::<_, String>(0)?, "agentId": r.get::<_, String>(1)?})))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The live bot made from one preset, if any. An archived one does not
    /// count: archiving frees the preset so it can be hired again.
    pub fn agent_with_preset(&self, preset_id: &str) -> Result<Option<Value>, StorageError> {
        let id: Option<String> = {
            let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
            c.query_row("SELECT id FROM agents WHERE preset_id=?1 AND archived=0", [preset_id], |r| r.get(0)).optional()?
        };
        match id {
            Some(id) => self.agent_get(&id).map(Some),
            None => Ok(None),
        }
    }

    /// Records which catalogue employee a bot was made from. `None` clears it.
    pub fn agent_set_preset(&self, id: &str, preset_id: Option<&str>) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let changed = c.execute(
            "UPDATE agents SET preset_id=?2,updated_at=?3 WHERE id=?1",
            params![id, preset_id, Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)],
        )?;
        if changed == 0 {
            return Err(StorageError::AgentNotFound);
        }
        Ok(())
    }
}
