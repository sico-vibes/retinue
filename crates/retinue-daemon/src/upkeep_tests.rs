// 20 November 2026: the background cleanup, the size report and the one
// owner-started compaction. Included into the daemon's own test module.

#[tokio::test]
async fn storage_maintenance_reports_the_file_both_jobs_and_the_retention() {
    let (st, _) = state();
    let report = dispatch(&st, "storage.maintenance", json!({})).await.unwrap();
    assert!(report["fileBytes"].as_u64().is_some_and(|bytes| bytes > 0), "{report}");
    assert!(["none", "full", "incremental"].contains(&report["autoVacuum"].as_str().unwrap()), "{report}");
    assert_eq!(report["eventRetentionDays"], 3);
    assert!(report["compaction"]["estimateSeconds"].as_u64().unwrap() >= 5);
    assert!(report["compaction"]["requiredFreeBytes"].as_u64().is_some());
    assert!(report["sizes"]["toolCalls"]["rows"].as_u64().is_some(), "the per-table sizes are reported");
    assert!(report["sizes"]["events"]["bytes"].as_u64().is_some());
    assert!(report["toolSlim"].is_object() || report["toolSlim"].is_null());
}

#[tokio::test]
async fn compacting_the_database_needs_the_owners_confirmation() {
    let (st, _) = state();
    let refused = dispatch(&st, "storage.vacuum", json!({})).await.unwrap_err();
    assert_eq!(refused.1.code, "invalid_argument", "without confirm nothing is rewritten");
    // The in-memory test database makes the work trivial; the reply still names
    // both sizes and the mode it leaves behind.
    let done = dispatch(&st, "storage.vacuum", json!({"confirm":true})).await.unwrap();
    assert!(done["beforeBytes"].as_u64().is_some() && done["afterBytes"].as_u64().is_some(), "{done}");
    assert_eq!(done["autoVacuum"], "incremental");
}

#[test]
fn the_compaction_estimate_is_bounded() {
    assert_eq!(super::upkeep::compaction_seconds(0), 5);
    assert_eq!(super::upkeep::compaction_seconds(1_300_000_000), 61);
    assert_eq!(super::upkeep::compaction_seconds(1_000_000_000_000), 900);
}

#[tokio::test]
async fn the_first_background_pass_backs_up_and_slims_old_rows() {
    let env = crate::test_env::TestEnv::new("upkeep");
    let path = env.data_root().join("retinue.db");
    let (st, _events) = state_with(Storage::open(&path).unwrap());
    st.db.create_session("s-upkeep", "rt-test", "codex", ".", "Upkeep").unwrap();
    // A row exactly as the version before this wave wrote it, through its own
    // connection: the storage crate no longer writes a payload at all.
    let data = json!({
        "id": "old", "sessionId": "s-upkeep", "kind": "edit", "title": "Edit", "state": "completed",
        "raw": {"type":"tool_use","name":"Edit","input":{"file_path":"a.ts","old_string":"x".repeat(300_000)}},
        "startedAt": "2026-11-01T10:00:00.000Z",
        "detail": {"path": "a.ts", "label": "Edited a.ts"},
    });
    {
        let writer = rusqlite::Connection::open(&path).unwrap();
        writer.execute("INSERT INTO tool_calls(id,session_id,turn_id,state,data) VALUES('old','s-upkeep',NULL,'completed',?1)", [data.to_string()]).unwrap();
    }
    assert_eq!(st.db.slim_candidates().unwrap(), 1);
    super::upkeep::slim_old_rows(&st, &path).await;
    let backups = cleanup_backups(&env.data_root().join("backups"));
    assert_eq!(backups, 1, "one verified backup is written before the first row changes");
    let row: String = rusqlite::Connection::open(&path).unwrap()
        .query_row("SELECT data FROM tool_calls WHERE id='old'", [], |r| r.get(0)).unwrap();
    assert!(!row.contains("\"raw\""), "the provider payload is gone");
    assert!(row.contains("\"path\":\"a.ts\""), "the detail the tool view shows stays");
    assert!(row.len() < 2_000, "the row is now the size of what the UI reads: {row}");
    assert_eq!(st.db.slim_candidates().unwrap(), 0);
    // A second pass has nothing to do, so it takes no second backup.
    super::upkeep::slim_old_rows(&st, &path).await;
    assert_eq!(cleanup_backups(&env.data_root().join("backups")), 1);
}

fn cleanup_backups(folder: &std::path::Path) -> usize {
    std::fs::read_dir(folder).map(|entries| entries.flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("retinue-pre-cleanup") && name.ends_with(".db")
        })
        .count()).unwrap_or(0)
}
