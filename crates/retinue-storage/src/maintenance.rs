//! Keeping the database small after the fact (20 November 2026).
//!
//! The database grew to 1.3 GB because two things were written for ever:
//! `tool_calls.data.raw` held every provider's own payload (982 MB in 2,410
//! edit rows; the largest 2.4 MB), and `app_events` was never trimmed
//! (166 MB). New rows are already slim (`lib.rs`, `upsert_tool_in_turn`), so
//! this module handles what is already stored:
//!
//! * `slim_storage_batch` strips the provider payload from old `tool_calls`,
//!   `file_changes` and `permission_requests` rows and caps what is left. It
//!   works in rowid batches bounded by rows and by bytes read, and records its
//!   cursor, so the lock is held briefly, an interrupted run resumes and a
//!   finished run touches nothing again.
//! * `trim_events_batch` drops events the window no longer replays. The window
//!   loads `app.snapshot` and then replays from that snapshot's sequence, and a
//!   gap (`replayAvailable: false`) makes it load a fresh snapshot, so an event
//!   log older than a few days is not needed by any reader. Three event types
//!   are never trimmed, because storage reads them for as long as the row they
//!   describe exists: `turn.error` (a turn's detail), and the newest
//!   `session.context.updated` / `session.context.reset` (a conversation's
//!   context counters).
//! * `compact` is the one full `VACUUM` that both gives the freed pages back to
//!   the disk and switches the file to incremental auto-vacuum. It is an owner
//!   action (Doctor), never automatic: it rewrites the whole file and needs
//!   about its own size in free space. `incremental_vacuum` then returns a few
//!   pages at a time in the background.
//!
//! Nothing here runs inside `Storage::open`. The daemon starts it from a
//! background task after its ready line (`crates/retinue-daemon/src/upkeep.rs`).

use super::*;

/// The row cap of one cleanup batch. The byte cap below is the bound that
/// matters on the owner's database, where an old edit row averaged about
/// 400 KB; 200 stays as a ceiling, so a table of tiny rows still moves in
/// transactions of a sensible size.
pub const SLIM_BATCH: usize = 200;
/// The byte cap of one cleanup batch: about 4 MB of rows are read and rewritten
/// while the storage lock is held, so a message, tool or approval write waits
/// for a fraction of a second instead of for 80 MB at 13 MB/s. At least one
/// row is always taken, so a single oversized row still advances the cursor.
pub const SLIM_BATCH_BYTES: u64 = 4 * 1024 * 1024;
/// Events dropped per transaction. The trim selects only `sequence` and deletes
/// by primary key, so its lock time does not grow with an event's payload; 500
/// is fine for the small payloads the log carries.
pub const EVENT_TRIM_BATCH: usize = 500;
/// How long the event log is kept. The window and the phone replay from a fresh
/// snapshot, so this only has to cover a socket that dropped and reconnected;
/// three days also covers a weekend. T3 Code keeps about one day.
pub const EVENT_RETENTION_DAYS: i64 = 3;
/// Pages returned per background step of `PRAGMA incremental_vacuum` (4 KB
/// pages: 8 MB a step).
pub const VACUUM_STEP_PAGES: u32 = 2048;

const SLIM_KEY: &str = "internal.maintenance.slim.v1";
const EVENTS_KEY: &str = "internal.maintenance.events.v1";
/// The event types a reader still asks for, by name or payload, after the window
/// has moved on.
const PROTECTED_EVENTS: &str = "'turn.error','session.context.updated','session.context.reset'";
/// The three tables that once carried a provider payload beside their detail.
const SLIM_TABLES: [(&str, &str); 3] = [("tool_calls", "tool"), ("file_changes", "file"), ("permission_requests", "permission")];

/// One batch of the old-row cleanup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SlimStep {
    /// `tool`, `file`, `permission` or `done`.
    pub stage: String,
    pub scanned: u64,
    pub changed: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub done: bool,
}

/// One batch of the event-log trim.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventStep {
    pub scanned: u64,
    pub deleted: u64,
    /// The highest sequence this pass may look at: a fixed ceiling, so events
    /// written while the pass runs are left for the next one.
    pub through: i64,
    pub done: bool,
}

/// What one database costs on disk now, and what it could give back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StorageStats {
    pub database_bytes: u64,
    pub page_size: u64,
    pub page_count: u64,
    pub freelist_pages: u64,
    /// 0 none, 1 full, 2 incremental.
    pub auto_vacuum: u8,
    pub tool_rows: u64,
    pub tool_bytes: u64,
    pub file_rows: u64,
    pub file_bytes: u64,
    pub permission_bytes: u64,
    pub event_rows: u64,
    pub event_bytes: u64,
    pub message_bytes: u64,
    pub turn_bytes: u64,
    pub session_bytes: u64,
}

impl Storage {
    fn progress(&self, key: &str) -> Result<Value, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let raw: Option<String> = c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).optional()?;
        Ok(raw.and_then(|text| serde_json::from_str::<Value>(&text).ok()).unwrap_or_else(|| json!({})))
    }

    fn save_progress(&self, key: &str, value: &Value) -> Result<(), StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        c.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value.to_string()])?;
        Ok(())
    }

    /// The cleanup's own record: what it has done so far, for a restart and for
    /// the Doctor. It holds counts only.
    pub fn slim_progress(&self) -> Result<Value, StorageError> {
        self.progress(SLIM_KEY)
    }

    /// The event trim's own record. `cursor` is where an interrupted pass
    /// stopped; `doneAt` marks a finished pass and clears the cursor.
    pub fn event_trim_progress(&self) -> Result<Value, StorageError> {
        self.progress(EVENTS_KEY)
    }

    /// How many rows still carry a provider payload or exceed the row cap, so
    /// the caller can take one verified backup and then know when to stop
    /// asking. Counts only.
    pub fn slim_candidates(&self) -> Result<u64, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut total = 0u64;
        for (table, _) in SLIM_TABLES {
            let sql = format!(
                "SELECT COUNT(*) FROM {table} WHERE json_valid(data) \
                 AND (json_extract(data,'$.raw') IS NOT NULL OR length(CAST(data AS BLOB))>?1)"
            );
            total += c.query_row(&sql, [TOOL_ROW_BYTES as i64], |r| r.get::<_, i64>(0))?.max(0) as u64;
        }
        Ok(total)
    }

    /// A verified copy of the database, taken once before the cleanup changes
    /// its first row. It uses the same copy, integrity check and row-count
    /// manifest as a migration's backup, under `backups/retinue-pre-cleanup-*`.
    pub fn pre_cleanup_backup(&self, db_path: &Path) -> Result<Value, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let version: i64 = c.query_row("SELECT COALESCE(MAX(version),0) FROM schema_migrations", [], |r| r.get(0))?;
        let path = verified_backup_named(&c, db_path, version, "retinue-pre-cleanup")?;
        let bytes = fs::metadata(&path).map(|entry| entry.len()).unwrap_or(0);
        Ok(json!({"backupPath": path.to_string_lossy().into_owned(), "bytes": bytes, "schemaVersion": version}))
    }

    /// One batch of the old-row cleanup, resuming from the recorded cursor.
    /// A batch stops at `batch` rows or at `SLIM_BATCH_BYTES` of stored rows,
    /// whichever comes first, and always takes at least one row. The stage
    /// advances when neither bound stopped the batch (the table's candidates
    /// are exhausted); the cursor advances to the last rowid examined, changed
    /// or not, so no row is visited twice.
    pub fn slim_storage_batch(&self, batch: usize) -> Result<SlimStep, StorageError> {
        if batch == 0 { return Err(StorageError::InvalidAgent); }
        let progress = self.progress(SLIM_KEY)?;
        let mut stage = progress["stage"].as_u64().unwrap_or(0) as usize;
        if stage >= SLIM_TABLES.len() {
            return Ok(SlimStep { stage: "done".into(), done: true, ..SlimStep::default() });
        }
        let mut cursor = progress["cursor"].as_i64().unwrap_or(0);
        let mut rows = progress["rows"].as_u64().unwrap_or(0);
        let mut before = progress["bytesBefore"].as_u64().unwrap_or(0);
        let mut after = progress["bytesAfter"].as_u64().unwrap_or(0);
        let mut step = SlimStep { stage: SLIM_TABLES[stage].1.to_owned(), ..SlimStep::default() };
        let mut read_bytes = 0u64;
        {
            let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
            let (table, _) = SLIM_TABLES[stage];
            // The stored size is selected beside the text, so the byte bound is
            // decided before a row is parsed. A row is a candidate when it still
            // carries `raw` or is larger than the row cap.
            let sql = format!(
                "SELECT rowid, data, length(CAST(data AS BLOB)) FROM {table} WHERE rowid>?1 AND json_valid(data) \
                 AND (json_extract(data,'$.raw') IS NOT NULL OR length(CAST(data AS BLOB))>?3) \
                 ORDER BY rowid LIMIT ?2"
            );
            let mut q = c.prepare(&sql)?;
            let mut query = q.query(params![cursor, batch as i64, TOOL_ROW_BYTES as i64])?;
            let mut found = Vec::new();
            while let Some(row) = query.next()? {
                let rowid: i64 = row.get(0)?;
                let text: String = row.get(1)?;
                let size: i64 = row.get(2)?;
                read_bytes = read_bytes.saturating_add(size.max(0) as u64);
                found.push((rowid, text));
                // Checked after the push, so one oversized row always advances.
                if found.len() >= batch || read_bytes >= SLIM_BATCH_BYTES { break; }
            }
            drop(query);
            drop(q);
            step.scanned = found.len() as u64;
            for (rowid, text) in &found {
                cursor = *rowid;
                let Ok(mut data) = serde_json::from_str::<Value>(text) else { continue };
                let size_before = text.len() as u64;
                slim_stored_row(&mut data);
                let slim = data.to_string();
                let size_after = slim.len() as u64;
                if slim != *text {
                    c.execute(&format!("UPDATE {table} SET data=?2 WHERE rowid=?1"), params![rowid, slim])?;
                    step.changed += 1;
                    step.bytes_before += size_before;
                    step.bytes_after += size_after;
                }
            }
            // A batch stopped by the byte bound may hold the table's last row;
            // the next call then reads nothing and moves the stage on.
            if found.len() < batch && read_bytes < SLIM_BATCH_BYTES {
                stage += 1;
                cursor = 0;
            }
        }
        rows += step.changed;
        before += step.bytes_before;
        after += step.bytes_after;
        if stage >= SLIM_TABLES.len() {
            step.stage = "done".into();
            step.done = true;
        }
        self.save_progress(SLIM_KEY, &json!({
            "stage": stage,
            "cursor": cursor,
            "rows": rows,
            "bytesBefore": before,
            "bytesAfter": after,
            "updatedAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }))?;
        Ok(step)
    }

    /// One batch of the event-log trim, up to `through`. `done` is true when a
    /// batch came back short, so the caller stops and waits for the next pass.
    /// The newest events are always left behind, so `replay_events` never sees
    /// an empty log for a client that is connected right now.
    pub fn trim_events_batch(&self, batch: usize, through: i64) -> Result<EventStep, StorageError> {
        if batch == 0 { return Err(StorageError::InvalidAgent); }
        let cutoff = (Utc::now() - Duration::days(EVENT_RETENTION_DAYS)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let progress = self.progress(EVENTS_KEY)?;
        let mut cursor = progress["cursor"].as_i64().unwrap_or(0).min(through);
        let mut step = EventStep { through, ..EventStep::default() };
        {
            let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
            let sql = format!(
                "SELECT sequence FROM app_events WHERE sequence>?1 AND sequence<=?2 AND timestamp<?3 \
                 AND event_type NOT IN ({PROTECTED_EVENTS}) ORDER BY sequence LIMIT ?4"
            );
            let mut q = c.prepare(&sql)?;
            let ids = q.query_map(params![cursor, through, cutoff, batch as i64], |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
            step.scanned = ids.len() as u64;
            if let Some(last) = ids.last() {
                let placeholders = vec!["?"; ids.len()].join(",");
                let deleted = c.execute(&format!("DELETE FROM app_events WHERE sequence IN ({placeholders})"), rusqlite::params_from_iter(ids.iter()))?;
                step.deleted = deleted as u64;
                cursor = *last;
            }
            step.done = ids.len() < batch;
        }
        let record = if step.done {
            json!({"cursor": 0, "deleted": progress["deleted"].as_u64().unwrap_or(0) + step.deleted, "doneAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)})
        } else {
            json!({"cursor": cursor, "deleted": progress["deleted"].as_u64().unwrap_or(0) + step.deleted, "doneAt": progress["doneAt"].clone()})
        };
        self.save_progress(EVENTS_KEY, &record)?;
        Ok(step)
    }

    /// The newest sequence in the log, which bounds one trim pass.
    pub fn event_head(&self) -> Result<i64, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        Ok(c.query_row("SELECT COALESCE(MAX(sequence),0) FROM app_events", [], |r| r.get(0))?)
    }

    /// The file's shape only: SQLite's own page counts, so the Doctor can say
    /// how much compaction would give back without reading a single row.
    pub fn database_shape(&self) -> Result<Value, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let page_count: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let freelist: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let auto_vacuum: i64 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
        Ok(json!({
            "sizeBytes": page_size.max(0) * page_count.max(0),
            "pageSize": page_size.max(0),
            "pageCount": page_count.max(0),
            "freelistPages": freelist.max(0),
            "reclaimableBytes": page_size.max(0) * freelist.max(0),
            "autoVacuum": auto_vacuum.clamp(0, 2),
        }))
    }

    /// What the database holds and what it could give back. Counts and bytes
    /// only; no user data crosses this read.
    pub fn storage_stats(&self) -> Result<StorageStats, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let page_count: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let freelist: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        let auto_vacuum: i64 = c.query_row("PRAGMA auto_vacuum", [], |r| r.get(0))?;
        let mut stats = StorageStats {
            database_bytes: (page_size * page_count).max(0) as u64,
            page_size: page_size.max(0) as u64,
            page_count: page_count.max(0) as u64,
            freelist_pages: freelist.max(0) as u64,
            auto_vacuum: auto_vacuum.clamp(0, 2) as u8,
            ..StorageStats::default()
        };
        let (rows, bytes): (i64, i64) = c.query_row("SELECT COUNT(*), COALESCE(SUM(length(CAST(data AS BLOB))),0) FROM tool_calls", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        (stats.tool_rows, stats.tool_bytes) = (rows.max(0) as u64, bytes.max(0) as u64);
        let (rows, bytes): (i64, i64) = c.query_row("SELECT COUNT(*), COALESCE(SUM(length(CAST(data AS BLOB))),0) FROM file_changes", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        (stats.file_rows, stats.file_bytes) = (rows.max(0) as u64, bytes.max(0) as u64);
        stats.permission_bytes = c.query_row("SELECT COALESCE(SUM(length(CAST(data AS BLOB))),0) FROM permission_requests", [], |r| r.get::<_, i64>(0))?.max(0) as u64;
        let (rows, bytes): (i64, i64) = c.query_row("SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))),0) FROM app_events", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        (stats.event_rows, stats.event_bytes) = (rows.max(0) as u64, bytes.max(0) as u64);
        stats.message_bytes = c.query_row("SELECT COALESCE(SUM(length(CAST(content AS BLOB))+length(CAST(data AS BLOB))),0) FROM messages", [], |r| r.get::<_, i64>(0))?.max(0) as u64;
        stats.turn_bytes = c.query_row("SELECT COALESCE(SUM(length(CAST(data AS BLOB))),0) FROM turns", [], |r| r.get::<_, i64>(0))?.max(0) as u64;
        stats.session_bytes = c.query_row("SELECT COALESCE(SUM(length(CAST(data AS BLOB))),0) FROM sessions", [], |r| r.get::<_, i64>(0))?.max(0) as u64;
        Ok(stats)
    }

    /// The one full `VACUUM`: it rewrites the file, giving the freed pages back
    /// to the disk, and leaves the file in incremental auto-vacuum mode so
    /// later deletions can be returned a few pages at a time. It needs free
    /// space of about the file's own size, holds the write lock, and can take
    /// minutes on a gigabyte; only the owner starts it.
    pub fn compact_database(&self) -> Result<Value, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let before: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let started = std::time::Instant::now();
        // A reader (a snapshot) can hold the file for a moment; wait instead of
        // failing the owner's one compaction.
        c.busy_timeout(std::time::Duration::from_secs(30))?;
        let outcome = c.execute_batch("PRAGMA auto_vacuum=INCREMENTAL; VACUUM; PRAGMA wal_checkpoint(TRUNCATE);");
        let _ = c.busy_timeout(std::time::Duration::ZERO);
        outcome?;
        let after: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        Ok(json!({
            "beforeBytes": before.max(0) * page_size.max(0),
            "afterBytes": after.max(0) * page_size.max(0),
            "millis": started.elapsed().as_millis() as u64,
        }))
    }

    /// A few pages back to the disk, only meaningful once the file is in
    /// incremental auto-vacuum mode (`compact_database` is what switches it).
    /// Returns the pages actually freed, so the caller stops when it is zero.
    pub fn incremental_vacuum(&self, pages: u32) -> Result<u64, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let before: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        c.execute_batch(&format!("PRAGMA incremental_vacuum({})", pages.max(1)))?;
        let after: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        Ok(before.saturating_sub(after).max(0) as u64)
    }
}

/// One stored row as it is now kept: no provider payload, every content field
/// clipped with an honest note, and the whole row under `TOOL_ROW_BYTES`.
fn slim_stored_row(row: &mut Value) {
    let Some(object) = row.as_object_mut() else { return };
    object.remove("raw");
    let long = object.get("detail").and_then(Value::as_str).filter(|detail| detail.chars().count() > DETAIL_TEXT_LIMIT).map(str::to_owned);
    if let Some(detail) = long { object.insert("detail".into(), json!(clip_body(&detail, DETAIL_TEXT_LIMIT))); }
    cap_row(row, TOOL_ROW_BYTES);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Storage { Storage::open_in_memory().unwrap() }

    fn session(db: &Storage, id: &str) {
        db.upsert_runtime(&json!({"id":"rt-maint","provider":"codex"})).unwrap();
        db.create_session(id, "rt-maint", "codex", ".", "Maintenance").unwrap();
    }

    /// A row exactly as the version before 20 November 2026 wrote it.
    fn legacy_tool(db: &Storage, session_id: &str, id: &str, raw: Value, detail: Value) {
        let data = json!({
            "id": id, "sessionId": session_id, "kind": "edit", "title": "Edit", "state": "completed",
            "raw": raw, "startedAt": "2026-11-01T10:00:00.000Z", "completedAt": "2026-11-01T10:00:01.000Z",
            "detail": detail,
        });
        let c = db.conn.lock().unwrap();
        c.execute("INSERT INTO tool_calls(id,session_id,turn_id,state,data) VALUES(?1,?2,NULL,'completed',?3)", params![id, session_id, data.to_string()]).unwrap();
    }

    fn stored(db: &Storage, table: &str, id: &str) -> Value {
        let c = db.conn.lock().unwrap();
        let text: String = c.query_row(&format!("SELECT data FROM {table} WHERE id=?1"), [id], |r| r.get(0)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn stored_file(db: &Storage, path: &str) -> Value {
        let c = db.conn.lock().unwrap();
        let text: String = c.query_row("SELECT data FROM file_changes WHERE path=?1", [path], |r| r.get(0)).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn event(db: &Storage, kind: &str, payload: Value, at: &str) {
        let c = db.conn.lock().unwrap();
        c.execute("INSERT INTO app_events(event_id,timestamp,event_type,payload) VALUES(?1,?2,?3,?4)",
            params![Uuid::new_v4().to_string(), at, kind, payload.to_string()]).unwrap();
    }

    fn old_stamp() -> String { (Utc::now() - Duration::days(EVENT_RETENTION_DAYS + 1)).to_rfc3339_opts(SecondsFormat::Millis, true) }

    #[test]
    fn every_provider_keeps_a_bounded_detail_and_never_the_raw_payload() {
        let db = db();
        session(&db, "s-providers");
        let huge = 2_400_000;
        let cases = [
            ("claude", json!({"type":"tool_use","name":"Edit","input":{"file_path":"src/a.ts","old_string":"x".repeat(huge),"new_string":"y"}})),
            ("codex", json!({"type":"commandExecution","command":"npm test","aggregatedOutput":"z".repeat(huge),"exitCode":0})),
            ("opencode", json!({"sessionUpdate":"tool_call","toolCallId":"c1","title":"Edit","rawInput":{"filePath":"src/a.ts"},"content":[{"type":"content","content":{"type":"text","text":"w".repeat(huge)}}]})),
            ("pi", json!({"name":"write","path":"src/a.ts","content":"v".repeat(huge)})),
        ];
        for (provider, raw) in cases {
            let id = format!("call-{provider}");
            let tool = db.upsert_tool_in_turn("s-providers", None, &id, "edit", "Edit", "completed", &raw, None).unwrap();
            assert!(tool.get("raw").is_none(), "{provider}: the provider payload is not stored");
            let row = stored(&db, "tool_calls", &id);
            assert!(row.get("raw").is_none(), "{provider}: the stored row has no raw payload");
            assert!(row["payloadBytes"].as_u64().unwrap() >= huge as u64, "{provider}: the payload's size is kept");
            assert!(serde_json::to_vec(&row).unwrap().len() <= TOOL_ROW_BYTES, "{provider}: the row stays under the cap");
            assert!(!row["detail"].as_object().unwrap().is_empty(), "{provider}: the tool view still has a detail");
        }
    }

    #[test]
    fn an_oversized_output_says_how_much_was_not_kept() {
        let db = db();
        session(&db, "s-output");
        let raw = json!({"type":"commandExecution","command":"cat big.log","aggregatedOutput":"x".repeat(DETAIL_TEXT_LIMIT + 5_000)});
        let tool = db.upsert_tool_in_turn("s-output", None, "big", "commandExecution", "Ran cat big.log", "completed", &raw, None).unwrap();
        let output = tool["detail"]["output"].as_str().unwrap();
        assert!(output.chars().count() <= DETAIL_TEXT_LIMIT, "the field keeps its cap");
        assert!(output.contains("truncated"), "{output}");
        assert!(output.contains("KB not kept"), "{output}");
        assert!(tool["detail"]["command"] == "cat big.log", "the command itself is unchanged");
    }

    #[test]
    fn a_row_never_exceeds_the_row_cap_whatever_the_characters() {
        let db = db();
        session(&db, "s-row");
        // Six fields at the field limit, each of four-byte characters: 96 KB.
        let wide: String = "😀".repeat(DETAIL_TEXT_LIMIT);
        let raw = json!({
            "type": "tool_result",
            "command": wide, "aggregatedOutput": wide, "result": wide, "error": wide,
            "progress": wide, "contentItems": [{"text": wide}],
            "arguments": {"blob": wide},
        });
        let tool = db.upsert_tool_in_turn("s-row", None, "wide", "exec", "Ran wide", "completed", &raw, None).unwrap();
        assert!(serde_json::to_vec(&tool).unwrap().len() <= TOOL_ROW_BYTES, "{} bytes", serde_json::to_vec(&tool).unwrap().len());
        assert_eq!(tool["detailTruncated"], true, "the row says it was trimmed");
        let output = tool["detail"]["output"].as_str().unwrap();
        assert!(output.contains("truncated"), "the output still says what was dropped: {output}");
    }

    #[test]
    fn updating_an_old_row_drops_its_payload_too() {
        let db = db();
        session(&db, "s-append");
        legacy_tool(&db, "s-append", "old", json!({"aggregatedOutput":"x".repeat(500_000)}), json!({"command":"npm test"}));
        let updated = db.append_tool_output("old", "a sub-agent's words").unwrap().unwrap();
        assert!(updated.get("raw").is_none(), "the row is slimmed when it is next written");
        assert!(updated["detail"]["output"].as_str().unwrap().contains("a sub-agent's words"));
    }

    #[test]
    fn the_cleanup_resumes_never_rewrites_a_clean_row_and_leaves_messages_alone() {
        let dir = std::env::temp_dir().join(format!("retinue-maint-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("retinue.db");
        {
            let db = Storage::open(&path).unwrap();
            session(&db, "s-clean");
            db.append_message("s-clean", "t", "user", "a message that must survive").unwrap();
            for index in 0..5 {
                legacy_tool(&db, "s-clean", &format!("legacy-{index}"), json!({"aggregatedOutput":"x".repeat(200_000)}), json!({"command":"npm test"}));
            }
            db.upsert_tool_in_turn("s-clean", None, "clean", "commandExecution", "Ran ls", "completed", &json!({"command":"ls","aggregatedOutput":"a.txt"}), None).unwrap();
            assert_eq!(db.slim_candidates().unwrap(), 5, "the clean row and the message are not candidates");
            let first = db.slim_storage_batch(2).unwrap();
            assert_eq!((first.stage.as_str(), first.changed), ("tool", 2));
            db.slim_progress().unwrap();
        }
        // A restart continues from the recorded cursor.
        let db = Storage::open(&path).unwrap();
        let mut steps = 0;
        loop {
            steps += 1;
            let step = db.slim_storage_batch(2).unwrap();
            assert!(steps < 20, "the cleanup terminates");
            if step.done { break; }
        }
        assert_eq!(db.slim_candidates().unwrap(), 0, "every row that carried a payload was rewritten");
        let progress = db.slim_progress().unwrap();
        assert_eq!(progress["stage"], 3);
        assert_eq!(progress["rows"], 5, "five rows changed, once each");
        let clean_before = stored(&db, "tool_calls", "clean");
        let again = db.slim_storage_batch(2).unwrap();
        assert!(again.done && again.changed == 0, "a finished cleanup touches nothing again");
        assert_eq!(stored(&db, "tool_calls", "clean"), clean_before);
        assert_eq!(db.session_detail("s-clean").unwrap()["messages"][0]["content"], "a message that must survive");
        drop(db);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_batch_of_large_rows_stops_at_the_byte_bound_and_resumes_at_the_next_row() {
        let db = db();
        session(&db, "s-byte-batch");
        // Six rows of about 1.1 MB: the 4 MB bound can take only four of them,
        // although the 200-row cap would take all six.
        for index in 0..6 {
            legacy_tool(&db, "s-byte-batch", &format!("large-{index}"), json!({"aggregatedOutput":"x".repeat(1_100_000)}), json!({"command":"npm test"}));
        }
        let first = db.slim_storage_batch(SLIM_BATCH).unwrap();
        assert_eq!(first.stage, "tool");
        assert_eq!(first.scanned, 4, "four rows are about 4.4 MB; the byte bound stops the batch");
        assert!(first.bytes_before >= SLIM_BATCH_BYTES, "the batch rewrote at least the byte bound: {}", first.bytes_before);
        assert!(!first.done);
        let progress = db.slim_progress().unwrap();
        assert_eq!(progress["cursor"], 4, "the cursor is the last rowid examined, so the next batch starts at row five");
        let mut scanned = first.scanned;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 10, "the cleanup terminates");
            let step = db.slim_storage_batch(SLIM_BATCH).unwrap();
            scanned += step.scanned;
            if step.done { break; }
        }
        assert_eq!(scanned, 6, "the second call took the two remaining rows, and no row was visited twice");
        assert_eq!(db.slim_candidates().unwrap(), 0, "every large row was slimmed");
        assert_eq!(db.slim_progress().unwrap()["rows"], 6);
    }

    #[test]
    fn the_cleanup_handles_a_fixture_database_written_before_it_existed() {
        let dir = std::env::temp_dir().join(format!("retinue-fixture-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("retinue.db");
        let backup;
        {
            let db = Storage::open(&path).unwrap();
            session(&db, "s-fixture");
            legacy_tool(&db, "s-fixture", "edit-1", json!({"type":"tool_use","name":"Edit","input":{"file_path":"a.ts","old_string":"x".repeat(1_800_000)}}), json!({"path":"a.ts","label":"Edited a.ts"}));
            db.insert_file_in_turn("s-fixture", None, "a.ts", &json!({"changes":[{"path":"a.ts","kind":"update","diff":"+x".repeat(120_000)}]})).unwrap();
            db.insert_permission("perm-1", "s-fixture", "req-1", "Write a.ts", Some("Writes a file."), &["allow".into(), "deny".into()], &json!({"request":{"input":{"content":"y".repeat(900_000)}}}), &json!({})).unwrap();
            backup = db.pre_cleanup_backup(&path).unwrap();
            assert!(Path::new(backup["backupPath"].as_str().unwrap()).is_file(), "the verified copy exists");
            assert!(backup["bytes"].as_u64().unwrap() > 0);
        }
        let db = Storage::open(&path).unwrap();
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 30);
            if db.slim_storage_batch(10).unwrap().done { break; }
        }
        let tool = stored(&db, "tool_calls", "edit-1");
        assert!(tool.get("raw").is_none());
        assert_eq!(tool["detail"]["path"], "a.ts");
        let file = stored_file(&db, "a.ts");
        assert!(file.get("raw").is_none(), "a file change's payload is not kept either");
        assert!(serde_json::to_vec(&file).unwrap().len() <= TOOL_ROW_BYTES);
        let card = stored(&db, "permission_requests", "perm-1");
        assert!(card.get("raw").is_none());
        assert_eq!(card["choices"], json!(["allow","deny"]), "the card still shows its choices");
        drop(db);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_event_trim_keeps_what_readers_still_ask_for() {
        let db = db();
        session(&db, "s-events");
        db.create_turn("t-1", "s-events").unwrap();
        let old = old_stamp();
        event(&db, "message.delta", json!({"sessionId":"s-events","delta":"old"}), &old);
        event(&db, "tool.changed", json!({"sessionId":"s-events","detail":{}}), &old);
        event(&db, "turn.error", json!({"turnId":"t-1","detail":{"message":"failed"}}), &old);
        event(&db, "session.context.updated", json!({"sessionId":"s-events","contextUsed":10}), &old);
        event(&db, "session.context.reset", json!({"sessionId":"s-events"}), &old);
        let recent = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        event(&db, "session.changed", json!({"id":"s-events"}), &recent);
        let through = db.event_head().unwrap();
        let mut guard = 0;
        let mut deleted = 0;
        loop {
            guard += 1;
            assert!(guard < 20);
            let step = db.trim_events_batch(1, through).unwrap();
            deleted += step.deleted;
            if step.done { break; }
        }
        assert_eq!(deleted, 2, "only the two old events with no reader are dropped");
        let c = db.conn.lock().unwrap();
        let kept: Vec<String> = c.prepare("SELECT event_type FROM app_events ORDER BY sequence").unwrap()
            .query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<Result<_, _>>().unwrap();
        for kind in ["turn.error", "session.context.updated", "session.context.reset", "session.changed"] {
            assert!(kept.iter().any(|event| event == kind), "{kind} is kept: {kept:?}");
        }
        assert!(!kept.iter().any(|event| event == "message.delta" || event == "tool.changed"), "{kept:?}");
        // The two readers behind the kept types still answer.
        drop(c);
        assert_eq!(db.session_context_state("s-events").unwrap(), (None, None, None, false), "a reset clears the counters");
        let detail = db.session_detail("s-events").unwrap();
        let turn = detail["turns"].as_array().unwrap().iter().find(|turn| turn["id"] == "t-1").expect("the turn");
        assert_eq!(turn["detail"]["message"], "failed", "a failure detail outlives the trim");
    }

    #[test]
    fn compaction_gives_pages_back_and_leaves_the_file_incremental() {
        let dir = std::env::temp_dir().join(format!("retinue-vacuum-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("retinue.db");
        let db = Storage::open(&path).unwrap();
        session(&db, "s-vacuum");
        for index in 0..12 {
            legacy_tool(&db, "s-vacuum", &format!("row-{index}"), json!({"aggregatedOutput":"x".repeat(120_000)}), json!({"command":"npm test"}));
        }
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 20);
            if db.slim_storage_batch(4).unwrap().done { break; }
        }
        let before = db.database_shape().unwrap();
        assert_eq!(before["autoVacuum"], 0, "a database written before this wave is not incremental");
        assert!(before["freelistPages"].as_u64().unwrap() > 0, "the cleanup left free pages inside the file");
        let done = db.compact_database().unwrap();
        assert!(done["afterBytes"].as_u64().unwrap() < done["beforeBytes"].as_u64().unwrap(), "{done}");
        let after = db.database_shape().unwrap();
        assert_eq!(after["autoVacuum"], 2, "the file is now in incremental auto-vacuum mode");
        assert_eq!(after["freelistPages"], 0, "everything was given back");
        assert_eq!(db.incremental_vacuum(VACUUM_STEP_PAGES).unwrap(), 0);
        drop(db);
        let _ = fs::remove_dir_all(&dir);
    }
}