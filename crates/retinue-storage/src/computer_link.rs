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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
