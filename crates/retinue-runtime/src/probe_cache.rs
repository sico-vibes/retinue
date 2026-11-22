//! The version and protocol probes, remembered per binary build (21 November
//! 2026, performance item 7).
//!
//! Discovery asks each installed CLI two cheap questions by starting it: what
//! version it is (`--version`) and whether it speaks the protocol Retinue needs
//! (`--help`, or `app-server --help`). Those answers only change when the binary
//! changes, so they are kept here between launches, keyed on the resolved
//! executable plus its size and modification time — Pandacode's approach
//! (`docs/research/performance-2026-11.md` §3.6, §6.7).
//!
//! * A **hit** needs all three to match. A replaced or rebuilt CLI is a miss and
//!   is probed again, and an explicit scan (`runtime.refresh {force:true}`)
//!   ignores the cache entirely.
//! * **Failures are never cached.** A protocol check that failed is retried on
//!   the next scan, exactly as before.
//! * The answer's **age** is carried to the runtime row (`probeCachedAgeMs`), so
//!   Settings can say how old the check is.
//! * **Nothing here is a secret.** The file holds executable paths, sizes,
//!   modification times, version labels and one boolean per binary.
//!
//! The sign-in state is *not* cached: it changes without the binary changing,
//! and a stale "signed in" would be a lie. Only the two probes above are.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Bumped when the file's meaning changes; an older file is ignored, not read.
const CACHE_VERSION: u32 = 1;

/// One binary's answer, with the identity it was read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeEntry {
    /// The resolved executable, as the runtime row names it.
    pub path: String,
    pub size: u64,
    /// Modification time in Unix milliseconds. `-1` means it could not be read,
    /// and such an entry never matches.
    pub modified_ms: i64,
    pub version: Option<String>,
    /// The protocol check passed when this entry was written. Entries that did
    /// not pass are never stored.
    pub protocol_ok: bool,
    /// When the version and protocol probes actually ran (Unix milliseconds).
    /// Filling in a version that failed then does not move it.
    pub checked_at_ms: i64,
}

impl ProbeEntry {
    /// How old this answer was when it was served, in milliseconds.
    pub fn age_ms(&self, now_ms: i64) -> u64 {
        now_ms.saturating_sub(self.checked_at_ms).max(0) as u64
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeFile {
    version: u32,
    entries: BTreeMap<String, ProbeEntry>,
}

/// The cache in memory, with the file it was loaded from (if any).
#[derive(Debug, Default)]
pub struct ProbeCache {
    path: Option<PathBuf>,
    entries: BTreeMap<String, ProbeEntry>,
}

/// The key a binary is cached under. Windows paths are case-insensitive, so the
/// key is lowercased; the entry keeps the path as it was resolved.
fn key_of(executable: &Path) -> String {
    executable.to_string_lossy().to_lowercase()
}

/// Size and modification time, both read from the file the CLI actually runs.
/// `None` when the file cannot be read (then nothing is cached for it).
pub fn file_identity(executable: &Path) -> Option<(u64, i64)> {
    let metadata = std::fs::metadata(executable).ok()?;
    let modified = metadata.modified().ok()?;
    let modified_ms = modified.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((metadata.len(), modified_ms))
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as i64).unwrap_or(0)
}

impl ProbeCache {
    /// Loads the cache at `path`; a missing or unreadable file is an empty cache,
    /// never an error, so a broken cache cannot stop a start.
    pub fn at(path: Option<PathBuf>) -> Self {
        let entries = path.as_deref().and_then(read_entries).unwrap_or_default();
        Self { path, entries }
    }

    /// A cache that is never written: used where a scan needs no memory (tests,
    /// and `discover()` keeping its old signature).
    pub fn empty() -> Self {
        Self::default()
    }

    fn file(&self) -> ProbeFile {
        ProbeFile { version: CACHE_VERSION, entries: self.entries.clone() }
    }

    /// The entry for this binary, only when it describes the file as it is now
    /// and its protocol check passed. `force` is the caller's business: an
    /// explicit scan simply does not call this.
    pub fn lookup(&self, executable: &Path) -> Option<&ProbeEntry> {
        let entry = self.entries.get(&key_of(executable))?;
        if !entry.protocol_ok {
            return None;
        }
        let identity = file_identity(executable)?;
        (identity == (entry.size, entry.modified_ms)).then_some(entry)
    }

    /// Keeps an answer that was actually probed now. Entries whose protocol check
    /// failed are dropped rather than stored.
    pub fn record(&mut self, entry: ProbeEntry) {
        let key = entry.path.to_lowercase();
        if !entry.protocol_ok {
            self.entries.remove(&key);
        } else {
            self.entries.insert(key, entry);
        }
    }

    /// Forgets binaries this scan did not see, so the file stays bounded to what
    /// is installed. A provider that is switched off is not resolved at all, so
    /// turning it back on probes it once.
    pub fn retain_seen(&mut self, seen: &BTreeSet<String>) {
        self.entries.retain(|key, _| seen.contains(key));
    }

    /// How many binaries this scan saw, for the log line and the example.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Writes the cache where it was loaded from, replacing the file in one
    /// rename so a crash mid-write cannot leave half a file. A cache with no
    /// path is memory-only.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.file()).map_err(std::io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path)
    }
}

fn read_entries(path: &Path) -> Option<BTreeMap<String, ProbeEntry>> {
    let text = std::fs::read_to_string(path).ok()?;
    let file: ProbeFile = serde_json::from_str(&text).ok()?;
    (file.version == CACHE_VERSION).then_some(file.entries)
}

/// Counts the CLI processes one scan started, so the daemon's log and the
/// timing example can say what a launch cost. One atomic add per spawn.
#[derive(Debug, Clone, Default)]
pub struct SpawnCounter(Arc<AtomicU64>);

impl SpawnCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// A counter nobody reads, for a probe outside a scan.
    pub fn detached() -> Self {
        Self::default()
    }

    pub fn count(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &Path, size: u64, modified_ms: i64) -> ProbeEntry {
        ProbeEntry { path: path.to_string_lossy().into_owned(), size, modified_ms, version: Some("1.2.3".into()), protocol_ok: true, checked_at_ms: now_ms() }
    }

    fn fixture(name: &str, bytes: &[u8]) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("retinue-probe-cache-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("cli.exe");
        std::fs::write(&file, bytes).unwrap();
        (dir, file)
    }

    #[test]
    fn a_hit_needs_the_same_path_size_and_modification_time() {
        let (dir, file) = fixture("identity", b"first build");
        let (size, modified) = file_identity(&file).unwrap();
        let mut cache = ProbeCache::empty();
        cache.record(entry(&file, size, modified));
        assert!(cache.lookup(&file).is_some(), "an unchanged binary is served from the cache");

        // A rebuilt binary of the same size has a new modification time.
        cache.record(entry(&file, size, modified + 1));
        assert!(cache.lookup(&file).is_none(), "a changed modification time is a miss");

        // A different size is a miss even under the same path.
        cache.record(entry(&file, size + 1, modified));
        assert!(cache.lookup(&file).is_none(), "a changed size is a miss");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_protocol_check_is_never_cached_and_the_age_is_reported() {
        let (dir, file) = fixture("failures", b"binary");
        let (size, modified) = file_identity(&file).unwrap();
        let mut cache = ProbeCache::empty();
        let mut failed = entry(&file, size, modified);
        failed.protocol_ok = false;
        cache.record(failed);
        assert!(cache.is_empty(), "a failure is retried next scan");
        assert!(cache.lookup(&file).is_none());

        let stale = entry(&file, size, modified);
        let age = stale.age_ms(stale.checked_at_ms + 5_000);
        assert_eq!(age, 5_000);
        assert_eq!(stale.age_ms(stale.checked_at_ms - 5_000), 0, "a clock that went backwards is not negative");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_cache_round_trips_through_its_file_and_drops_what_a_scan_did_not_see() {
        let (dir, file) = fixture("round-trip", b"binary bytes");
        let path = dir.join("runtime-probes.json");
        let (size, modified) = file_identity(&file).unwrap();
        let first = {
            let mut cache = ProbeCache::at(Some(path.clone()));
            cache.record(entry(&file, size, modified));
            cache.save().unwrap();
            cache.lookup(&file).cloned()
        };
        // A second launch reads the same answer without a CLI being started.
        let mut second = ProbeCache::at(Some(path.clone()));
        assert_eq!(second.lookup(&file).cloned(), first);
        assert_eq!(second.len(), 1);
        // A scan that did not see this binary forgets it.
        second.retain_seen(&BTreeSet::new());
        assert!(second.is_empty());
        second.save().unwrap();
        assert!(ProbeCache::at(Some(path)).is_empty(), "the emptied cache is what the next launch reads");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_or_future_file_is_an_empty_cache_not_an_error() {
        let (dir, _file) = fixture("corrupt", b"binary");
        let path = dir.join("runtime-probes.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(ProbeCache::at(Some(path.clone())).is_empty());
        std::fs::write(&path, r#"{"version":99,"entries":{}}"#).unwrap();
        assert!(ProbeCache::at(Some(path)).is_empty(), "a file from a newer shape is ignored");
        let _ = std::fs::remove_dir_all(dir);
    }
}
