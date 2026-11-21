//! The database's own health, kept off the start-up path (21 November 2026).
//!
//! `Storage::open` must not read the whole file before the daemon serves it. On
//! the owner's 1.3 GB file that was most of a start (`lessons.md`, 2026-11-20:
//! `schema_v12_valid` ran `PRAGMA integrity_check` and `PRAGMA foreign_key_check`
//! over every page before the ready line). The guarantee it gave — refuse a
//! corrupt file before serving it — is kept in three parts:
//!
//! 1. **Before the ready line, only what is cheap and bounded:** the file's
//!    100-byte header (magic and page size), the schema shape (the ledger,
//!    `user_version`, `sqlite_master` and the one value constraint each version
//!    checks), and a full check *only* when the marker below says the last one
//!    failed. `PRAGMA quick_check` is not used here: it walks every page just
//!    like `integrity_check` and measured as slow (2026-11-20; numbers in
//!    `lessons.md`), while the header, the page counts and the shape queries
//!    cost milliseconds even under load.
//! 2. **After the ready line, at low priority:** the full `integrity_check` and
//!    `foreign_key_check`, at most once a day or once per change to the file,
//!    on a connection of its own so no request waits behind it.
//! 3. **A marker in `integrity_checks`,** one row of time, result, file size and
//!    page count. Nothing else ever goes in it. A failed check is reported by
//!    the Doctor and in the log, and the next start refuses the file until a
//!    full check passes. Nothing here repairs, rewrites or deletes anything.
//!
//! A migration still checks its own result in full before serving (`migrate_*`
//! and `schema_vN_valid`): that is a once-per-update cost, not a per-start one.

use std::io::Read;
use std::path::Path;
use std::time::Instant;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{open_reader, Storage, StorageError};

/// The one-table, one-row marker. A table rather than a settings row: settings
/// reach the window and the Remote allowlist, and this is a fact about the file,
/// not a preference.
const MARKER_TABLE: &str = "integrity_checks";

/// How often the unchanged file is checked again.
const DAILY: Duration = Duration::hours(24);
/// A file that changed since the last check is checked again, but not more
/// often than this: a busy database changes its page count all day.
const CHANGE_RECHECK: Duration = Duration::hours(1);

/// The SQLite header's first 16 bytes, and the offsets of the page size.
const HEADER_BYTES: usize = 100;
const MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Creates the marker table. Idempotent and additive, like the other
/// `ensure_*` schemas; it runs on every open and changes nothing once it exists.
pub(crate) fn ensure_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {MARKER_TABLE}(
            id INTEGER PRIMARY KEY CHECK(id=1),
            checked_at TEXT NOT NULL,
            result TEXT NOT NULL CHECK(result IN ('ok','failed')),
            detail TEXT,
            size_bytes INTEGER NOT NULL,
            page_count INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL,
            origin TEXT NOT NULL CHECK(origin IN ('start','background'))
        );"
    ))?;
    Ok(())
}

/// The file itself, before SQLite sees it: 100 bytes at offset 0. A file
/// shorter than a header, a missing magic string or an impossible page size is
/// refused here in under a millisecond. Only a file that exists is checked; a
/// path Retinue is about to create is not.
pub(crate) fn check_file_header(path: &Path) -> Result<(), StorageError> {
    let length = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        // A missing file is the fresh-install path: `Connection::open` creates it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(StorageError::Io(error)),
    };
    if length == 0 {
        return Ok(());
    }
    if length < HEADER_BYTES as u64 {
        return Err(StorageError::Corrupt("the file is shorter than a database header".into()));
    }
    let mut header = [0u8; HEADER_BYTES];
    std::fs::File::open(path)?.read_exact(&mut header)?;
    if &header[..16] != MAGIC {
        return Err(StorageError::Corrupt("the file does not begin with a SQLite header".into()));
    }
    let page_size = u16::from_be_bytes([header[16], header[17]]);
    if !valid_page_size(page_size) {
        return Err(StorageError::Corrupt("the header's page size is not one SQLite allows".into()));
    }
    Ok(())
}

/// SQLite's own rule: a power of two from 512 to 32768, or the encoded 1 that
/// means a 64 KiB page.
fn valid_page_size(page_size: u16) -> bool {
    page_size == 1 || ((512..=32768).contains(&page_size) && page_size.is_power_of_two())
}

/// `(sizeBytes, pageCount)` from SQLite's own counters: two header reads, no
/// page walk. The same numbers the Doctor's storage row shows.
fn shape(c: &Connection) -> Result<(u64, u64), StorageError> {
    let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let page_count: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    Ok(((page_size.max(0) * page_count.max(0)) as u64, page_count.max(0) as u64))
}

/// What one full pass found: `true` with no detail, or the first line SQLite
/// printed about it.
struct Outcome {
    ok: bool,
    detail: Option<String>,
}

/// `PRAGMA integrity_check` and `PRAGMA foreign_key_check` over every page,
/// with the lines read one at a time so a badly damaged file cannot make this
/// hold a page of errors.
fn page_check(c: &Connection) -> Result<Outcome, StorageError> {
    let mut first: Option<String> = None;
    let mut lines = 0usize;
    {
        let mut statement = c.prepare("PRAGMA integrity_check")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            lines += 1;
            if first.is_none() {
                first = Some(row.get::<_, String>(0)?);
            }
        }
    }
    if lines == 1 && first.as_deref() == Some("ok") {
        let violations: i64 = c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        return Ok(if violations == 0 {
            Outcome { ok: true, detail: None }
        } else {
            Outcome { ok: false, detail: Some(format!("{violations} foreign key violation(s)")) }
        });
    }
    Ok(Outcome { ok: false, detail: Some(first.unwrap_or_else(|| "the check returned no answer".into())) })
}

/// A pass that ran into SQLite's own error is a failed check, not no check: a
/// damaged page can make the statement itself fail, and the next start must act
/// on that. Only the verdict is kept, never a row of data.
fn full_check(c: &Connection) -> Outcome {
    let outcome = page_check(c);
    #[cfg(test)]
    count_pass();
    match outcome {
        Ok(outcome) => outcome,
        Err(error) => Outcome { ok: false, detail: Some(format!("the check could not finish: {error}")) },
    }
}

// How many whole-file passes this thread ran. Tests count them to prove a
// steady start runs none; thread-local so a parallel test cannot add to the
// count of the one being measured.
#[cfg(test)]
thread_local! {
    static FULL_CHECKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_pass() {
    FULL_CHECKS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
pub(crate) fn passes_so_far() -> u64 {
    FULL_CHECKS.with(std::cell::Cell::get)
}

/// A whole-file pass, for a migration that must check its own result before it
/// serves the database it just changed. The error text is not carried: the
/// caller only decides whether the file may be served.
pub(crate) fn passes(c: &Connection) -> bool {
    full_check(c).ok
}

fn record(c: &Connection, outcome: &Outcome, duration_ms: u64, origin: &str) -> Result<(), StorageError> {
    let (size_bytes, page_count) = shape(c)?;
    let checked_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    c.execute(
        &format!(
            "INSERT INTO {MARKER_TABLE}(id,checked_at,result,detail,size_bytes,page_count,duration_ms,origin)
             VALUES(1,?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET checked_at=excluded.checked_at,result=excluded.result,detail=excluded.detail,
                 size_bytes=excluded.size_bytes,page_count=excluded.page_count,duration_ms=excluded.duration_ms,origin=excluded.origin"
        ),
        params![checked_at, if outcome.ok { "ok" } else { "failed" }, outcome.detail, size_bytes as i64, page_count as i64, duration_ms as i64, origin],
    )?;
    Ok(())
}

/// The marker row as stored: `(checked_at, result, detail, size_bytes, page_count, duration_ms, origin)`.
type MarkerRow = (String, String, Option<String>, i64, i64, i64, String);

/// The marker, or `None` when no full check has run since this table existed.
pub(crate) fn marker(c: &Connection) -> Result<Option<Value>, StorageError> {
    let raw: Option<MarkerRow> = c
        .query_row(
            &format!("SELECT checked_at,result,detail,size_bytes,page_count,duration_ms,origin FROM {MARKER_TABLE} WHERE id=1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    Ok(raw.map(|(checked_at, result, detail, size_bytes, page_count, duration_ms, origin)| {
        json!({
            "checkedAt": checked_at,
            "result": result,
            "detail": detail,
            "sizeBytes": size_bytes,
            "pageCount": page_count,
            "durationMs": duration_ms,
            "origin": origin,
        })
    }))
}

/// The steady path's one pre-ready check, after the schema shape has passed: a
/// previous full check that failed keeps the file refused until one passes.
/// Nothing is repaired when it does not pass; the file is simply not served.
pub(crate) fn gate_before_serving(c: &Connection) -> Result<(), StorageError> {
    ensure_schema(c)?;
    let Some(previous) = marker(c)? else { return Ok(()) };
    if previous["result"].as_str() == Some("ok") {
        return Ok(());
    }
    let started = Instant::now();
    let outcome = full_check(c);
    record(c, &outcome, started.elapsed().as_millis() as u64, "start")?;
    if outcome.ok {
        return Ok(());
    }
    Err(StorageError::Corrupt(outcome.detail.unwrap_or_else(|| "the full check failed".into())))
}

impl Storage {
    /// The last full check's marker, for the Doctor. Read-only, content-free.
    pub fn integrity_marker(&self) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        marker(&c)
    }

    /// Whether the full check is owed now: never checked, the last one failed,
    /// a day has passed, or the file changed (its size or page count) since a
    /// check more than an hour old. An in-memory database has no file to check.
    pub fn integrity_check_due(&self) -> Result<bool, StorageError> {
        self.integrity_check_due_at(Utc::now())
    }

    pub fn integrity_check_due_at(&self, now: DateTime<Utc>) -> Result<bool, StorageError> {
        if self.path.is_none() {
            return Ok(false);
        }
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let Some(previous) = marker(&c)? else { return Ok(true) };
        if previous["result"].as_str() != Some("ok") {
            return Ok(true);
        }
        let checked_at = previous["checkedAt"]
            .as_str()
            .and_then(|stamp| DateTime::parse_from_rfc3339(stamp).ok())
            .map(|stamp| stamp.with_timezone(&Utc));
        let Some(checked_at) = checked_at else { return Ok(true) };
        let age = now.signed_duration_since(checked_at);
        if age >= DAILY {
            return Ok(true);
        }
        if age >= CHANGE_RECHECK {
            let (size_bytes, page_count) = shape(&c)?;
            if previous["sizeBytes"].as_u64() != Some(size_bytes) || previous["pageCount"].as_u64() != Some(page_count) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Runs the full check on a connection of its own — a read-only one, so it
    /// never holds the writer and never queues a request behind it — records the
    /// marker and answers with it. `None` for an in-memory database.
    ///
    /// The caller runs this off the async runtime and after the ready line: on a
    /// gigabyte it reads the whole file and takes as long as the disk allows.
    pub fn run_integrity_check(&self) -> Result<Option<Value>, StorageError> {
        let Some(path) = self.path.clone() else { return Ok(None) };
        let started = Instant::now();
        let outcome = {
            let reader = open_reader(&path)?;
            full_check(&reader)
        };
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        ensure_schema(&c)?;
        record(&c, &outcome, started.elapsed().as_millis() as u64, "background")?;
        marker(&c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("retinue-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn checks() -> u64 {
        passes_so_far()
    }

    /// The point of the change: a start on a database that is already migrated
    /// runs no whole-file pass before it returns. The first open of a new file
    /// does (its migration checks its own result in full), and that is counted
    /// here to prove the counter is live.
    #[test]
    fn a_steady_start_runs_no_full_check_before_it_returns() {
        let dir = temp_dir("integrity-steady");
        let path = dir.join("retinue.db");
        let before = checks();
        drop(Storage::open(&path).unwrap());
        assert!(checks() > before, "the first open is a migration and checks in full");
        let steady = checks();
        {
            let db = Storage::open(&path).unwrap();
            assert!(db.integrity_marker().unwrap().is_none(), "a migration leaves no marker: the background check still owns the file");
        }
        assert_eq!(checks(), steady, "a steady start reads no page it does not need");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A failed full check is what the next start acts on: it runs the check
    /// again before the caller gets a database, and the marker says it passed.
    #[test]
    fn a_failed_marker_makes_the_next_start_check_before_it_returns() {
        let dir = temp_dir("integrity-refused");
        let path = dir.join("retinue.db");
        drop(Storage::open(&path).unwrap());
        {
            let c = Connection::open(&path).unwrap();
            c.execute(
                &format!("INSERT OR REPLACE INTO {MARKER_TABLE}(id,checked_at,result,detail,size_bytes,page_count,duration_ms,origin) VALUES(1,?1,'failed','database disk image is malformed',1,1,1,'background')"),
                [Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)],
            )
            .unwrap();
        }
        let before = checks();
        let db = Storage::open(&path).unwrap();
        assert_eq!(checks(), before + 1, "one full pass runs before the start serves the file");
        let marker = db.integrity_marker().unwrap().expect("the passing check leaves its marker");
        assert_eq!(marker["result"], "ok");
        assert_eq!(marker["origin"], "start");
        drop(db);
        assert!(!Storage::open(&path).unwrap().integrity_check_due().unwrap(), "and the background check is not due again");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The background pass records time, result, size and page count, and
    /// nothing that could carry the owner's words.
    #[test]
    fn the_background_check_records_a_content_free_marker() {
        let dir = temp_dir("integrity-marker");
        let path = dir.join("retinue.db");
        let db = Storage::open(&path).unwrap();
        assert!(db.integrity_check_due().unwrap(), "no marker yet: a check is owed");
        let marker = db.run_integrity_check().unwrap().expect("a file database is checked");
        assert_eq!(marker["result"], "ok");
        assert_eq!(marker["origin"], "background");
        assert!(marker["checkedAt"].as_str().is_some_and(|stamp| stamp.starts_with("20")));
        assert!(marker["sizeBytes"].as_u64().unwrap() >= 4096);
        assert!(marker["pageCount"].as_u64().unwrap() > 0);
        assert!(marker["detail"].is_null());
        let keys: Vec<&str> = marker.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys.len(), 7, "checkedAt, result, detail, sizeBytes, pageCount, durationMs, origin: {keys:?}");
        assert!(!db.integrity_check_due().unwrap(), "a fresh pass is not due again");
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Due means: a day passed, or the file changed and the last check is more
    /// than an hour old. An unchanged file an hour after its check is not due,
    /// and neither is an in-memory database, which has no file.
    #[test]
    fn a_check_is_due_after_a_day_or_after_a_changed_file() {
        let dir = temp_dir("integrity-due");
        let path = dir.join("retinue.db");
        let db = Storage::open(&path).unwrap();
        db.run_integrity_check().unwrap().unwrap();
        let now = Utc::now();
        assert!(!db.integrity_check_due_at(now + Duration::hours(2)).unwrap(), "unchanged, and less than a day");
        assert!(db.integrity_check_due_at(now + Duration::hours(25)).unwrap(), "a day has passed");
        // A change to the file's pages makes it due again after an hour.
        for index in 0..40 {
            db.push_event("test.row", &json!({"n": index, "body": "x".repeat(20_000)})).unwrap();
        }
        let (_, pages_after) = shape(&db.conn.lock().unwrap()).unwrap();
        assert!(pages_after > 0);
        assert!(!db.integrity_check_due_at(now + Duration::minutes(30)).unwrap(), "a change under an hour old waits");
        assert!(db.integrity_check_due_at(now + Duration::hours(2)).unwrap(), "a changed file is checked after an hour");
        drop(db);
        let memory = Storage::open_in_memory().unwrap();
        assert!(!memory.integrity_check_due().unwrap(), "nothing to check without a file");
        assert!(memory.run_integrity_check().unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A corrupt page is still refused before the file is served: the first
    /// start serves it (the page walk runs later, in the background), the pass
    /// notices, and every start after that one refuses until a check passes.
    #[test]
    fn a_corrupt_page_is_found_in_the_background_and_refused_at_the_next_start() {
        let dir = temp_dir("integrity-corrupt");
        let path = dir.join("retinue.db");
        let db = Storage::open(&path).unwrap();
        for index in 0..200 {
            db.push_event("test.row", &json!({"n": index, "body": "x".repeat(20_000)})).unwrap();
        }
        drop(db);
        let length = std::fs::metadata(&path).unwrap().len();
        let page_count = length / 4096;
        assert!(page_count > 200, "the fixture has enough pages to lose one: {page_count}");
        // Overwrite one page in the middle: large enough that the middle is data
        // and not the schema, which every open reads.
        let target = page_count * 3 / 5;
        {
            let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start(target * 4096)).unwrap();
            file.write_all(&[0xAB; 4096]).unwrap();
        }
        // No marker says anything is wrong yet, so this start serves the file;
        // the pass is what notices.
        let db = Storage::open(&path).unwrap();
        let marker = db.run_integrity_check().unwrap().expect("a file database is checked");
        assert_eq!(marker["result"], "failed", "{marker}");
        assert!(marker["detail"].as_str().is_some_and(|detail| !detail.is_empty()));
        drop(db);
        let refused = Storage::open(&path);
        assert!(matches!(refused, Err(StorageError::Corrupt(_))), "the next start refuses the file before serving it");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A file that is not a database at all is refused from its header, before
    /// SQLite is asked anything: 100 bytes, and no page walk.
    #[test]
    fn a_file_without_a_sqlite_header_is_refused_before_it_is_opened() {
        let dir = temp_dir("integrity-header");
        let garbage = dir.join("garbage.db");
        std::fs::write(&garbage, vec![0x2A; 4096]).unwrap();
        assert!(matches!(Storage::open(&garbage), Err(StorageError::Corrupt(_))));
        let short = dir.join("short.db");
        std::fs::write(&short, b"SQLite format 3").unwrap();
        assert!(matches!(Storage::open(&short), Err(StorageError::Corrupt(_))));
        // A path that does not exist yet is the fresh install, not an error.
        check_file_header(&dir.join("new.db")).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
