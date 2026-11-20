//! Keeping the stored database small, in the background (20 November 2026).
//!
//! The owner's database reached 1.3 GB: every tool call kept its provider's raw
//! payload for ever (982 MB in 2,410 edit rows) and the event log was never
//! trimmed (166 MB). New rows are slim at write time; this module deals with
//! what was already stored, and it is careful about when:
//!
//! * **Never inside `Storage::open` and never before the ready line.** Startup
//!   already misses the shell's ten-second limit on a busy disk; the cleanup
//!   would make that worse. `start_at_boot` is called once, after the ready
//!   line, and its first pass waits `FIRST_PASS` so the window can open.
//! * **A verified backup first.** The first pass copies the database with the
//!   storage crate's own verified backup (integrity check and row counts into a
//!   manifest under `backups/`), and skips the whole cleanup with one log line
//!   when the disk cannot hold the copy.
//! * **Small batches, resumable.** 200 rows a transaction with a short pause,
//!   and a cursor in `settings`, so a restart continues where it stopped.
//! * **Compaction is the owner's.** Giving the pages back needs one full
//!   `VACUUM`, which rewrites the file; that is offered as a Doctor action with
//!   a size and a time estimate and never run automatically. Once the file is
//!   in incremental auto-vacuum mode, `PRAGMA incremental_vacuum` returns a few
//!   pages per step here.

use std::path::Path;
use std::time::Duration;

use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use retinue_storage::maintenance::{EVENT_RETENTION_DAYS, EVENT_TRIM_BATCH, SLIM_BATCH, VACUUM_STEP_PAGES};
use serde_json::{json, Value};

use crate::{derr, AppState, DispatchError};

/// The first pass waits this long after the ready line, so opening the window
/// and the first snapshot never share the disk with the cleanup.
const FIRST_PASS: Duration = Duration::from_secs(45);
/// A short yield between batches: the point is to stay out of a turn's way.
const BETWEEN_BATCHES: Duration = Duration::from_millis(150);
/// After a finished pass, look again this often for events that have become
/// older than the retention window while the app stayed open.
const PASS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Free space kept beside the backup copy, on top of its own size.
const BACKUP_MARGIN_BYTES: u64 = 64 * 1024 * 1024;
/// The compaction estimate assumes this write speed; the owner's disk measured
/// slower under load, so it is deliberately conservative.
const COMPACTION_BYTES_PER_SECOND: u64 = 20 * 1024 * 1024;

/// Called once from `run()` after the ready line. It only spawns.
pub fn start_at_boot(st: &AppState) {
    let state = st.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_PASS).await;
        loop {
            pass(&state).await;
            tokio::time::sleep(PASS_INTERVAL).await;
        }
    });
}

async fn pass(st: &AppState) {
    slim_old_rows(st, &crate::paths::database()).await;
    trim_old_events(st).await;
    vacuum_steps(st).await;
}

/// Strip the provider payload from rows written before 20 November 2026, one
/// batch at a time. The verified backup is taken once, before the first row
/// changes, and a disk that cannot hold it skips the work with one log line.
pub(crate) async fn slim_old_rows(st: &AppState, database: &Path) {
    let remaining = match st.db.slim_candidates() {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(error = %error, "the tool-call cleanup could not count its rows");
            return;
        }
    };
    if remaining == 0 {
        return;
    }
    // `rows` is only non-zero once a row has changed, so this is the first pass.
    let started = st.db.slim_progress().ok().and_then(|progress| progress["rows"].as_u64()).unwrap_or(0);
    if started == 0 {
        let size = std::fs::metadata(database).map(|entry| entry.len()).unwrap_or(0);
        if let Some(free) = free_bytes(database) {
            if free < size + BACKUP_MARGIN_BYTES {
                tracing::warn!(
                    free_bytes = free,
                    database_bytes = size,
                    rows = remaining,
                    "tool-call cleanup skipped: not enough free disk space for the verified backup"
                );
                return;
            }
        }
        match st.db.pre_cleanup_backup(database) {
            Ok(backup) => {
                tracing::info!(backup = backup["backupPath"].as_str().unwrap_or(""), bytes = backup["bytes"].as_u64().unwrap_or(0), "tool-call cleanup: verified backup written");
            }
            Err(error) => {
                // The backup is the line that must not be crossed: without it,
                // nothing is rewritten.
                tracing::warn!(error = %error, rows = remaining, "tool-call cleanup skipped: the verified backup could not be written");
                return;
            }
        }
    }
    loop {
        match st.db.slim_storage_batch(SLIM_BATCH) {
            Ok(step) if step.done => {
                let progress = st.db.slim_progress().unwrap_or(Value::Null);
                tracing::info!(
                    rows = progress["rows"].as_u64().unwrap_or(0),
                    freed_bytes = progress["bytesBefore"].as_u64().unwrap_or(0).saturating_sub(progress["bytesAfter"].as_u64().unwrap_or(0)),
                    "tool-call cleanup finished"
                );
                break;
            }
            Ok(_) => tokio::time::sleep(BETWEEN_BATCHES).await,
            Err(error) => {
                tracing::warn!(error = %error, "tool-call cleanup stopped; it resumes after the next start");
                break;
            }
        }
    }
}

/// Drop events the window no longer replays. `turn.error` and both context
/// events are never dropped (storage still reads them for their row); everything
/// older than the retention window goes, in batches.
async fn trim_old_events(st: &AppState) {
    let through = match st.db.event_head() {
        Ok(head) => head,
        Err(error) => {
            tracing::warn!(error = %error, "the event-log trim could not read the newest sequence");
            return;
        }
    };
    if through == 0 {
        return;
    }
    loop {
        match st.db.trim_events_batch(EVENT_TRIM_BATCH, through) {
            Ok(step) if step.done => {
                if step.deleted > 0 {
                    tracing::info!(deleted = step.deleted, retention_days = EVENT_RETENTION_DAYS, "event log trimmed");
                }
                break;
            }
            Ok(_) => tokio::time::sleep(BETWEEN_BATCHES).await,
            Err(error) => {
                tracing::warn!(error = %error, "the event-log trim stopped; it resumes after the next start");
                break;
            }
        }
    }
}

/// Once the file is in incremental auto-vacuum mode, hand pages back a step at
/// a time. The cap (200 steps, about 1.6 GB) keeps a pass finite; another pass
/// continues after it.
async fn vacuum_steps(st: &AppState) {
    let Ok(shape) = st.db.database_shape() else { return };
    if shape["autoVacuum"].as_u64() != Some(2) || shape["freelistPages"].as_u64().unwrap_or(0) == 0 {
        return;
    }
    for _ in 0..200 {
        match st.db.incremental_vacuum(VACUUM_STEP_PAGES) {
            Ok(0) => break,
            Ok(pages) => {
                tracing::debug!(pages, "incremental vacuum returned pages to the disk");
                tokio::time::sleep(BETWEEN_BATCHES).await;
            }
            Err(error) => {
                tracing::warn!(error = %error, "incremental vacuum failed");
                break;
            }
        }
    }
}

/// A rough time for the one compaction: the file has to be read and written once.
pub(crate) fn compaction_seconds(bytes: u64) -> u64 {
    (bytes / COMPACTION_BYTES_PER_SECOND).clamp(5, 900)
}

/// What the Doctor and `storage.maintenance` report: the file's size, what
/// compaction would give back, both background jobs' progress, and the
/// compaction offer.
pub fn summary(st: &AppState) -> Result<Value, DispatchError> {
    let shape = st.db.database_shape().map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?;
    let size = shape["sizeBytes"].as_u64().unwrap_or(0);
    let reclaimable = shape["reclaimableBytes"].as_u64().unwrap_or(0);
    let mode = shape["autoVacuum"].as_u64().unwrap_or(0);
    let database = crate::paths::database();
    let free = free_bytes(&database);
    Ok(json!({
        "fileBytes": size,
        "pageBytes": shape["pageSize"],
        "pageCount": shape["pageCount"],
        "freelistPages": shape["freelistPages"],
        "reclaimableBytes": reclaimable,
        "autoVacuum": match mode { 1 => "full", 2 => "incremental", _ => "none" },
        "freeBytes": free,
        "compaction": {
            "needed": reclaimable >= 8 * 1024 * 1024,
            // A full VACUUM writes a new copy beside the old one.
            "requiredFreeBytes": size + BACKUP_MARGIN_BYTES,
            "estimateSeconds": compaction_seconds(size),
        },
        "toolSlim": st.db.slim_progress().unwrap_or(Value::Null),
        "eventTrim": st.db.event_trim_progress().unwrap_or(Value::Null),
        "eventRetentionDays": EVENT_RETENTION_DAYS,
        "checkedAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    }))
}

/// `storage.maintenance` (status) and `storage.vacuum` (the owner's compaction).
/// Both are desktop-only: sizes and the free-space reading are the owner's at
/// the computer, and the compaction rewrites the database file.
pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "storage.maintenance" => {
            let mut summary = summary(st)?;
            let stats = st.db.storage_stats().map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?;
            summary["sizes"] = json!({
                "toolCalls": {"rows": stats.tool_rows, "bytes": stats.tool_bytes},
                "fileChanges": {"rows": stats.file_rows, "bytes": stats.file_bytes},
                "permissions": {"bytes": stats.permission_bytes},
                "events": {"rows": stats.event_rows, "bytes": stats.event_bytes},
                "messages": {"bytes": stats.message_bytes},
                "turns": {"bytes": stats.turn_bytes},
                "sessions": {"bytes": stats.session_bytes},
            });
            Ok(summary)
        }
        "storage.vacuum" => {
            if p["confirm"] != true {
                return Err(derr("invalid_argument", "Compacting the database rewrites the file. Confirm it first.", StatusCode::BAD_REQUEST));
            }
            let shape = st.db.database_shape().map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?;
            let size = shape["sizeBytes"].as_u64().unwrap_or(0);
            let reclaimable = shape["reclaimableBytes"].as_u64().unwrap_or(0);
            let database = crate::paths::database();
            if let Some(free) = free_bytes(&database) {
                if free < size + BACKUP_MARGIN_BYTES {
                    return Err(derr(
                        "conflict",
                        "There is not enough free disk space for the compaction. Free some space and try again; nothing was changed.",
                        StatusCode::CONFLICT,
                    ));
                }
            }
            let db = st.db.clone();
            let done = tokio::task::spawn_blocking(move || db.compact_database())
                .await
                .map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?
                .map_err(|error| derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR))?;
            Ok(json!({
                "beforeBytes": done["beforeBytes"],
                "afterBytes": done["afterBytes"],
                "reclaimedBytes": reclaimable,
                "millis": done["millis"],
                "autoVacuum": "incremental",
            }))
        }
        _ => Err(derr("unsupported", "That storage method does not exist.", StatusCode::NOT_FOUND)),
    }
}

/// Free bytes on the volume that holds `path`. `None` where the platform has no
/// cheap answer for it; the verified backup's own failure is then the guard.
#[cfg(windows)]
pub(crate) fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives for the call,
    // and the two optional output pointers are null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub(crate) fn free_bytes(_path: &Path) -> Option<u64> {
    None
}
