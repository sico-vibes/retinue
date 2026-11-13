//! The linked VPS computer (Harbor): its address, whether employees get its
//! tools, and the *reference* to its bearer token in the credential store.
//!
//! The token itself is never in this table: `token_ref` names a
//! `Retinue/computer/<id>` credential, which only `secrets::store` reads. The
//! row never travels in a backup (nothing here is exported), and an import
//! unlinks the computer, so a restored machine starts with no credential.

use super::{params, Connection, Storage, StorageError};
use chrono::Utc;
use rusqlite::OptionalExtension;

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_computer_link_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS computer_link(
        id INTEGER PRIMARY KEY CHECK(id=1),
        address TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 0,
        token_ref TEXT NOT NULL,
        linked_at TEXT NOT NULL
    );",
    )?;
    // An employee's own switch (13 November 2026): the computer is this bot's
    // workplace, not just an occasional tool. Kept here, beside the link's
    // schema, so the agent columns other lanes touch stay as they are.
    let has_workplace: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('agents') WHERE name='computer_workplace')", [], |r| r.get(0))?;
    if !has_workplace { c.execute("ALTER TABLE agents ADD COLUMN computer_workplace INTEGER NOT NULL DEFAULT 0", [])?; }
    Ok(())
}

/// One linked computer. `enabled` is whether new conversations get its tools;
/// `token_ref` is the credential's name, never its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputerLink {
    pub address: String,
    pub enabled: bool,
    pub token_ref: String,
}

impl Storage {
    /// The linked computer, or `None` when none is linked.
    pub fn computer_link(&self) -> Result<Option<ComputerLink>, StorageError> {
        self.read_in(&self.read_conn, |c| {
            c.query_row("SELECT address,enabled,token_ref FROM computer_link WHERE id=1", [], |r| {
                Ok(ComputerLink { address: r.get(0)?, enabled: r.get::<_, i64>(1)? != 0, token_ref: r.get(2)? })
            })
            .optional()
            .map_err(StorageError::from)
        })
    }

    /// Replaces any link with `address` and `token_ref`, switched off until the
    /// owner turns it on.
    pub fn computer_link_set(&self, address: &str, token_ref: &str) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute(
            "INSERT INTO computer_link(id,address,enabled,token_ref,linked_at) VALUES(1,?1,0,?2,?3) ON CONFLICT(id) DO UPDATE SET address=excluded.address,enabled=0,token_ref=excluded.token_ref,linked_at=excluded.linked_at",
            params![address, token_ref, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn computer_link_set_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute("UPDATE computer_link SET enabled=?1 WHERE id=1", params![enabled])?;
        Ok(())
    }

    /// Removes the link. The caller deletes the credential it named.
    pub fn computer_link_clear(&self) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute("DELETE FROM computer_link", [])?;
        Ok(())
    }

    /// Takes a conversation's one claim to wake the computer before its first
    /// turn. True once per conversation, and a restart does not hand it out
    /// again: the marker lives in the session's own `data`, like `setupPending`,
    /// and is written in the same statement that tests it.
    pub fn session_take_computer_wake(&self, id: &str) -> Result<bool, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let changed = c.execute(
            "UPDATE sessions SET data=json_set(COALESCE(NULLIF(data,''),'{}'),'$.computerWoke',1) WHERE id=?1 AND json_extract(data,'$.computerWoke') IS NULL",
            [id],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn a_link_starts_switched_off_replaces_whole_and_clears() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(db.computer_link().unwrap(), None);
        db.computer_link_set("https://harbor.example.com", "computer/one").unwrap();
        let link = db.computer_link().unwrap().unwrap();
        assert_eq!((link.address.as_str(), link.enabled, link.token_ref.as_str()), ("https://harbor.example.com", false, "computer/one"), "off until the owner turns it on");
        db.computer_link_set_enabled(true).unwrap();
        assert!(db.computer_link().unwrap().unwrap().enabled);
        // A second link replaces the first, holds its own reference, and starts off.
        db.computer_link_set("http://127.0.0.1:8080", "computer/two").unwrap();
        let link = db.computer_link().unwrap().unwrap();
        assert_eq!((link.address.as_str(), link.enabled), ("http://127.0.0.1:8080", false));
        // Turning the switch on is idempotent and touches nothing else.
        db.computer_link_set_enabled(true).unwrap();
        db.computer_link_set_enabled(true).unwrap();
        assert_eq!(db.computer_link().unwrap().unwrap().token_ref, "computer/two");
        db.computer_link_clear().unwrap();
        assert_eq!(db.computer_link().unwrap(), None);
    }

    #[test]
    fn an_employees_workplace_switch_round_trips_and_its_wake_is_claimed_once() {
        let db = Storage::open_in_memory().unwrap();
        db.upsert_runtime(&json!({"id":"rt-test","provider":"codex","status":"offline"})).unwrap();
        let (worker, _) = db.agent_create(&json!({"name":"Worker","runtimeId":"rt-test","computerWorkplace":true})).unwrap();
        let id = worker["id"].as_str().unwrap();
        assert_eq!(worker["computerWorkplace"], true, "returned on create");
        assert_eq!(db.agent_get(id).unwrap()["computerWorkplace"], true, "returned on read");
        assert_eq!(db.agent_list(false, None).unwrap()[0]["computerWorkplace"], true, "returned on list");
        assert_eq!(db.agent_update(id, &json!({"computerWorkplace": false})).unwrap().0["computerWorkplace"], false, "a save turns it off");
        assert_eq!(db.agent_update(id, &json!({"computerWorkplace": Value::Null})).unwrap().0["computerWorkplace"], false, "null is off");
        // A bot created without the key is off, and the field is boolean only.
        assert_eq!(db.agent_create(&json!({"name":"Plain","runtimeId":"rt-test"})).unwrap().0["computerWorkplace"], false);
        assert!(db.agent_create(&json!({"name":"Odd","runtimeId":"rt-test","computerWorkplace":"yes"})).is_err());

        // The wake is claimed once per conversation, and each conversation has its own.
        db.create_session("s-wake", "rt-test", "codex", ".", "First").unwrap();
        db.create_session("s-other", "rt-test", "codex", ".", "Other").unwrap();
        assert!(db.session_take_computer_wake("s-wake").unwrap(), "the first turn takes the claim");
        assert!(!db.session_take_computer_wake("s-wake").unwrap(), "a later turn does not");
        assert!(db.session_take_computer_wake("s-other").unwrap(), "another conversation has its own");
        assert!(!db.session_take_computer_wake("no-such-session").unwrap());
    }
}
