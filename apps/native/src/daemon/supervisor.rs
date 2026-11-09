//! Finding and starting `retinued.exe`, ported from the Tauri shell's
//! supervisor (`apps/desktop/src-tauri/src/lib.rs:303` `candidate_daemon_paths`,
//! `:329` `start_daemon`).
//!
//! The preview starts the daemon as a child of its own process, gives it its own
//! database, reads the one bootstrap line and never prints the capability.

use std::fmt;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::ready::{parse_ready, ReadyError};

pub const DAEMON_EXE: &str = "retinued.exe";
// A copy of a large installed database opens and re-creates its read indexes on
// the first start, which measured 20 seconds on an 804 MB database; the line is
// still a timeout, not a hang, and a daemon that starts normally answers in
// seconds.
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(45);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The daemon's address and capability. The capability is used for the auth
/// header and nowhere else; `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
    address: String,
    capability: String,
}

impl Connection {
    pub fn new(address: impl Into<String>, capability: impl Into<String>) -> Self {
        Self { address: address.into(), capability: capability.into() }
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    /// Only for building the `Authorization` header.
    pub fn capability(&self) -> &str {
        &self.capability
    }

    pub fn rpc_url(&self) -> String {
        format!("http://{}/v1/rpc", self.address)
    }

    pub fn events_url(&self) -> String {
        format!("ws://{}/v1/events", self.address)
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Connection")
            .field("address", &self.address)
            .field("capability", &"<redacted>")
            .finish()
    }
}

/// A running daemon: the child process and how to reach it.
pub struct DaemonProcess {
    pub child: Child,
    pub connection: Connection,
}

impl DaemonProcess {
    /// Kill the child and wait for it. Safe to call more than once.
    pub fn shutdown(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where to look for the daemon, in the order the brief gives: beside our own
/// executable, then the workspace's release and debug builds.
pub fn daemon_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(folder) = executable.parent() {
            candidates.push(folder.join(DAEMON_EXE));
        }
    }
    if let Some(target) = std::env::var_os("CARGO_TARGET_DIR") {
        let target = PathBuf::from(target);
        candidates.push(target.join("release").join(DAEMON_EXE));
        candidates.push(target.join("debug").join(DAEMON_EXE));
    }
    if let Some(root) = workspace_root() {
        candidates.push(root.join("target").join("release").join(DAEMON_EXE));
        candidates.push(root.join("target").join("debug").join(DAEMON_EXE));
    }
    candidates
}

/// The repository this binary was built from. `CARGO_MANIFEST_DIR` is
/// `<repo>/apps/native` at compile time, which is how a dev build finds the
/// daemon it was built next to.
fn workspace_root() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("RETINUE_REPO_ROOT") {
        return Some(PathBuf::from(explicit));
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent()?.parent().map(Path::to_path_buf)
}

pub fn find_daemon() -> Result<PathBuf, String> {
    daemon_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            "retinued.exe is missing. Build the daemon first: cargo build -p retinue-daemon --release"
                .to_owned()
        })
}

/// Spawn the daemon, read its bootstrap line and keep draining both pipes.
pub fn spawn_daemon(executable: &Path, db_path: &Path) -> Result<DaemonProcess, String> {
    let mut command = Command::new(executable);
    command
        .env("RETINUE_PARENT_PID", std::process::id().to_string())
        .env("RETINUE_DB_PATH", db_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    // The hook bridge is installed beside the daemon, not beside its run copy, so
    // the daemon is told where it is (Health check, outside sessions).
    if let Some(folder) = executable.parent() {
        let bridge = folder.join("retinue-hook.exe");
        if bridge.is_file() {
            command.env("RETINUE_HOOK_BRIDGE", bridge);
        }
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", executable.display()))?;

    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Daemon bootstrap stream is unavailable.".to_owned());
    };
    let stderr = child.stderr.take();

    // The line is read on its own thread so the 45 second wait is a timeout and
    // not a hang. That thread keeps the pipe drained afterwards: a daemon whose
    // stdout buffer fills would block.
    let (bootstrap_tx, bootstrap_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader
            .read_line(&mut line)
            .map(|_| line)
            .map_err(|error| error.to_string());
        let _ = bootstrap_tx.send(result);
        for _line in reader.lines() {}
    });
    if let Some(stderr) = stderr {
        std::thread::spawn(move || for _line in BufReader::new(stderr).lines() {});
    }

    let line = match bootstrap_rx.recv_timeout(BOOTSTRAP_TIMEOUT) {
        Ok(Ok(line)) if !line.trim().is_empty() => line,
        Ok(Ok(_)) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Daemon exited before sending its bootstrap line.".to_owned());
        }
        Ok(Err(error)) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Could not read the daemon bootstrap: {error}"));
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Daemon did not send its bootstrap line within 45 seconds.".to_owned());
        }
    };

    let ready = match parse_ready(&line) {
        Ok(ready) => ready,
        Err(ReadyError::InvalidLine) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ReadyError::InvalidLine.to_string());
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
    };

    Ok(DaemonProcess {
        child,
        connection: Connection::new(ready.address, ready.capability),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemon_is_looked_for_beside_us_first() {
        let candidates = daemon_candidates();
        assert!(candidates.len() >= 2, "there must be more than one place to look");
        let first = &candidates[0];
        assert_eq!(first.file_name().unwrap(), DAEMON_EXE);
        assert!(first.parent().unwrap().is_absolute());
        // Beside the executable comes before any build folder.
        assert!(first.parent().and_then(Path::file_name).is_some());
    }

    #[test]
    fn a_connection_never_prints_its_capability() {
        let connection = Connection::new("127.0.0.1:49152", "do-not-print-me");
        let debug = format!("{connection:?}");
        assert!(!debug.contains("do-not-print-me"));
        assert!(debug.contains("<redacted>"));
        assert_eq!(connection.rpc_url(), "http://127.0.0.1:49152/v1/rpc");
        assert_eq!(connection.events_url(), "ws://127.0.0.1:49152/v1/events");
    }
}
