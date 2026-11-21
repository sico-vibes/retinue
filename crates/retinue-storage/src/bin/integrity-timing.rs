//! What a start-up database check costs on a large file (21 November 2026).
//!
//! The start-up path used to run `PRAGMA integrity_check` and
//! `PRAGMA foreign_key_check` over every page before the daemon's ready line.
//! This probe writes a database shaped like the owner's (one conversation per
//! file is not the point; the point is a gigabyte in tool payloads), then times
//! every check the start-up path could run against it, one at a time, so the
//! change can be judged on numbers rather than on the shape of the code.
//!
//! Usage:
//!   integrity-timing --db <path> [--megabytes 1024] [--no-seed] [--report <json>]
//!
//! `--no-seed` measures an existing file and never rewrites it. It prints one
//! JSON object: each check's milliseconds and the two `Storage::open` timings.
//! Nothing here touches the owner's database; give it a file of its own.

use retinue_storage::Storage;
use rusqlite::{params, Connection};
use serde_json::json;
use std::io::Read;
use std::path::Path;
use std::time::Instant;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|value| value == name).and_then(|at| args.get(at + 1)).cloned()
}

fn positive(args: &[String], name: &str, fallback: u64) -> Result<u64, String> {
    match arg(args, name) {
        None => Ok(fallback),
        Some(value) => match value.parse::<u64>() {
            Ok(parsed) if parsed > 0 => Ok(parsed),
            _ => Err(format!("{name} must be a positive number")),
        },
    }
}

fn millis(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

/// One gigabyte of tool output in rows of about 100 KB, the shape the owner's
/// file grew into (`lessons.md`, 2026-11-20). It goes through `Storage::open`
/// first so the schema is the real one, then writes payloads directly: the
/// storage API clips large rows now, and the point here is the file's size.
/// The rows name no real conversation, so the probe's `foreignKeyRows` counts
/// them and is expected to be large; it times the check, it does not certify
/// the fixture.
fn seed(path: &Path, megabytes: u64) -> Result<(), Box<dyn std::error::Error>> {
    drop(Storage::open(path)?);
    let bytes = megabytes * 1024 * 1024;
    let row_bytes = 100 * 1024u64;
    let rows = (bytes / row_bytes).max(1);
    let payload = "x".repeat(row_bytes as usize);
    let data = json!({"detail": payload}).to_string();
    let mut c = Connection::open(path)?;
    c.pragma_update(None, "foreign_keys", "OFF")?;
    c.pragma_update(None, "synchronous", "OFF")?;
    let tx = c.transaction()?;
    for index in 0..rows {
        tx.execute(
            "INSERT INTO tool_calls(id,session_id,turn_id,state,data) VALUES(?1,?2,NULL,'completed',?3)",
            params![format!("perf-tool-{index:06}"), format!("perf-session-{:04}", index % 40), data],
        )?;
    }
    tx.commit()?;
    let _: i64 = c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
    Ok(())
}

fn header(path: &Path) -> u64 {
    let started = Instant::now();
    let mut file = std::fs::File::open(path).expect("the probe file exists");
    let mut head = [0u8; 100];
    let read = file.read(&mut head).expect("the first page is readable");
    assert!(read == 100 && &head[..16] == b"SQLite format 3\0", "the header is a SQLite header");
    millis(started)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = arg(&args, "--db").ok_or("usage: integrity-timing --db <path> [--megabytes 1024] [--rows 10240] [--no-seed]")?;
    let path = Path::new(&path);
    let megabytes = positive(&args, "--megabytes", 1024)?;
    let report = arg(&args, "--report");
    if !args.iter().any(|value| value == "--no-seed") {
        seed(path, megabytes)?;
    }
    let size = std::fs::metadata(path)?.len();
    let mut timings = serde_json::Map::new();
    let first_started = Instant::now();
    drop(Storage::open(path)?);
    timings.insert("openFirst".into(), json!(millis(first_started)));
    let second_started = Instant::now();
    drop(Storage::open(path)?);
    timings.insert("openSecond".into(), json!(millis(second_started)));
    timings.insert("header".into(), json!(header(path)));
    let c = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let started = Instant::now();
    let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let size_of_page: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let freelist: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
    timings.insert("headerPragmas".into(), json!(millis(started)));
    let started = Instant::now();
    let tables: i64 = c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
    let ledger: i64 = c.query_row("SELECT COALESCE(MAX(version),0) FROM schema_migrations", [], |r| r.get(0))?;
    timings.insert("schemaShape".into(), json!(millis(started)));
    let started = Instant::now();
    let keys: i64 = c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
    timings.insert("foreignKeyCheck".into(), json!(millis(started)));
    let started = Instant::now();
    let quick: String = c.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    timings.insert("quickCheck".into(), json!(millis(started)));
    let started = Instant::now();
    let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    timings.insert("integrityCheck".into(), json!(millis(started)));
    println!(
        "{}",
        json!({
            "db": path.display().to_string(),
            "sizeBytes": size,
            "pageCount": pages,
            "pageSize": size_of_page,
            "freelistPages": freelist,
            "schemaObjects": tables,
            "ledger": ledger,
            "quickCheck": quick,
            "foreignKeyRows": keys,
            "integrity": integrity,
            "millis": timings,
        })
    );
    if let Some(report) = report {
        std::fs::write(report, json!({"sizeBytes":size,"millis":timings}).to_string())?;
    }
    Ok(())
}
