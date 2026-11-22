//! The console client is reached from `daemon_main`'s argument match, before a daemon
//! starts. This proves the reach for every verb: with no `console.json` beside the
//! database, `retinued <verb>` answers the client's own sentence and exits 3. A verb
//! added to `console::run_cli` but not to that match compiles, then starts a daemon
//! here and hangs until the limit below (20 November 2026, the first draft of
//! `retinued settings` had exactly that bug).
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_secs(20);

/// Runs `retinued` with the arguments and a data folder that holds no console file.
/// Returns the exit code (`None` if it did not exit in time), its output and a note
/// when the process had to be killed.
fn run_client(args: &[&str]) -> (Option<i32>, String, String, bool) {
    let folder = std::env::temp_dir().join(format!("retinue-console-cli-{}-{}", std::process::id(), args.join("-").replace(['/', '\\', ' '], "_")));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_retinued"))
        .args(args)
        .env("RETINUE_DB_PATH", folder.join("retinue.db"))
        // A host with no window, and no console file, which is the client's own case.
        .env_remove("RETINUE_PARENT_PID")
        .env_remove("RETINUE_HEADLESS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the retinued binary starts");
    let deadline = Instant::now() + LIMIT;
    let mut killed = false;
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status.code();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            killed = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let output = child.wait_with_output().unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    (code, String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned(), killed)
}

#[test]
fn every_console_verb_is_reached_before_a_daemon_starts() {
    let verbs: [&[&str]; 6] = [
        &["settings", "get"],
        &["settings", "set", "projects.devices.enabled", "true"],
        &["agents"],
        // 21 November 2026: the headless install verb, added beside `agents`.
        &["agents", "install", "claude"],
        &["bot", "list"],
        &["status"],
    ];
    for args in verbs {
        let (code, out, err, killed) = run_client(args);
        assert!(!killed, "{args:?} started a daemon instead of running the console client: stdout {out} stderr {err}");
        assert_eq!(code, Some(3), "{args:?} exited {code:?} instead of 3: stdout {out} stderr {err}");
        assert!(err.contains("No headless daemon is running here."), "{args:?} did not print the client's own sentence: {err}");
    }
}
