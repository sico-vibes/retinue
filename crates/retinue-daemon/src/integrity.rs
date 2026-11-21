//! The full database check, after the ready line (21 November 2026).
//!
//! `Storage::open` no longer runs `PRAGMA integrity_check` and
//! `PRAGMA foreign_key_check` over every page before the daemon serves
//! anything: on the owner's 1.3 GB file that was most of a start
//! (`lessons.md`, 2026-11-20). The guarantee stays, but it is paid for once a
//! day rather than once a start:
//!
//! * **After the ready line, and not immediately.** `start_at_boot` is called
//!   once, beside `upkeep::start_at_boot`, and its first look waits
//!   [`FIRST_PASS`] so the window is open and the first snapshot is done.
//! * **At low priority.** The check runs on a blocking thread set to below
//!   normal priority, on its own read-only connection, so no request waits
//!   behind it and the runtime is never blocked.
//! * **Once a day, or once per change to the file.** The due question is the
//!   storage layer's (`Storage::integrity_check_due`), answered from the marker
//!   the last check left: no marker, a failed one, a day passed, or the file's
//!   size or page count moved since a check more than an hour old.
//! * **A failure is honest and quiet.** One content-free log line (result, size,
//!   page count, duration) and the Doctor's `database.integrity` row. Nothing is
//!   repaired, rewritten or deleted; the marker makes the next start run the
//!   full check before the ready line again, and refuse the file until it passes
//!   (`retinue_storage::integrity::gate_before_serving`).

use std::time::Duration;

use crate::AppState;

/// The first look waits this long after the ready line: long enough that the
/// window's first snapshot and the saved state are not sharing the disk with a
/// whole-file read.
const FIRST_PASS: Duration = Duration::from_secs(90);
/// How often to ask again. The marker decides whether a check is owed, so this
/// is only how quickly a day-old marker is noticed while the app stays open.
const PASS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

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

/// One look: is the check owed, and if so, run it. Separate from the loop so a
/// test can ask once without waiting [`FIRST_PASS`].
pub(crate) async fn pass(st: &AppState) {
    let due = match st.db.integrity_check_due() {
        Ok(due) => due,
        Err(error) => {
            tracing::warn!(error = %error, "the database check could not read its marker");
            return;
        }
    };
    if !due {
        return;
    }
    check_now(st).await;
}

/// Runs the full check once, off the async threads, and logs its result. The
/// marker itself is written by the storage layer, in the same pass.
pub(crate) async fn check_now(st: &AppState) {
    let db = st.db.clone();
    let started = std::time::Instant::now();
    let outcome = tokio::task::spawn_blocking(move || {
        below_normal_priority();
        db.run_integrity_check()
    })
    .await;
    let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    match outcome {
        Ok(Ok(Some(marker))) => {
            let size_bytes = marker["sizeBytes"].as_u64();
            let page_count = marker["pageCount"].as_u64();
            if marker["result"].as_str() == Some("ok") {
                tracing::info!(elapsed_ms = elapsed_ms, size_bytes = size_bytes, page_count = page_count, "the database full check finished");
            } else {
                // The next start checks before it serves, and refuses the file
                // until a full check passes. Nothing is repaired here.
                tracing::error!(elapsed_ms = elapsed_ms, size_bytes = size_bytes, page_count = page_count, "the database full check failed");
            }
        }
        Ok(Ok(None)) => {}
        Ok(Err(error)) => tracing::warn!(error = %error, "the database full check could not run"),
        Err(_) => tracing::warn!("the database full check did not finish"),
    }
}

/// The reading is a whole-file pass; keep it out of the owner's way on the disk
/// as well as on the CPU. Windows only: the other platforms schedule a blocking
/// thread below interactive work already.
#[cfg(windows)]
fn below_normal_priority() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadPriority(handle: *mut std::ffi::c_void, priority: i32) -> i32;
    }
    // THREAD_PRIORITY_BELOW_NORMAL
    const BELOW_NORMAL: i32 = -1;
    unsafe {
        SetThreadPriority(GetCurrentThread(), BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
fn below_normal_priority() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase2a_tests::state_with;
    use retinue_storage::Storage;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("retinue-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("retinue.db")
    }

    /// A due check runs, records its marker, and leaves nothing that could carry
    /// the owner's words: a time, a result, a size and a page count.
    #[tokio::test]
    async fn a_due_check_runs_off_the_start_path_and_records_a_content_free_marker() {
        let path = temp_path("integrity-background");
        let (st, _) = state_with(Storage::open(&path).unwrap());
        assert!(st.db.integrity_marker().unwrap().is_none(), "a fresh database has no marker yet");
        pass(&st).await;
        let marker = st.db.integrity_marker().unwrap().expect("the pass records its marker");
        assert_eq!(marker["result"], "ok");
        assert_eq!(marker["origin"], "background");
        assert!(marker["checkedAt"].as_str().is_some_and(|stamp| stamp.contains('T')));
        assert!(marker["sizeBytes"].as_u64().unwrap() > 0);
        assert!(marker["pageCount"].as_u64().unwrap() > 0);
        assert!(marker["detail"].is_null(), "a clean check carries no detail");
        let keys: Vec<&str> = marker.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys.len(), 7, "time, result, detail, size, pages, duration, origin: {keys:?}");
        // A second look a moment later is not due, so nothing runs.
        assert!(!st.db.integrity_check_due().unwrap());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The check runs once and only once per due window: a second look right
    /// after the first writes nothing and runs nothing.
    #[tokio::test]
    async fn a_recorded_check_is_not_run_again_on_the_next_look() {
        let path = temp_path("integrity-once");
        let (st, _) = state_with(Storage::open(&path).unwrap());
        check_now(&st).await;
        let first = st.db.integrity_marker().unwrap().unwrap();
        assert_eq!(first["result"], "ok", "the fixture file is clean");
        assert!(!st.db.integrity_check_due().unwrap());
        pass(&st).await;
        assert_eq!(st.db.integrity_marker().unwrap().unwrap(), first, "a look that is not due leaves the marker alone");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
