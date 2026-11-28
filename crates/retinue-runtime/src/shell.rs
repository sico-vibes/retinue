//! The user's login shell environment, read once at start (28 November 2026,
//! Linux port).
//!
//! A daemon started by a desktop shell, a `.desktop` launcher or a systemd unit
//! inherits that launcher's `PATH`, not the one `~/.profile`, `~/.bashrc`,
//! `~/.zshrc` or a version manager builds. Claude Code, Codex and OpenCode
//! installed by npm, bun, fnm, volta or mise then live in `~/.local/bin`,
//! `~/.bun/bin` or a version-manager folder that the inherited `PATH` does not
//! name, so discovery finds nothing and the window says a CLI is missing that the
//! owner uses every day. T3 Code solves this the same way: run the login shell
//! once and merge the `PATH` it reports.
//!
//! The rules kept here:
//!
//! * No shell text is built from anything the owner or a project supplies: the
//!   command is one fixed string and the shell is one of three fixed paths.
//! * The whole probe has a 5-second limit and its output is never logged; only
//!   the merged `PATH` is kept, and the caller may log that it changed.
//! * The login shell's `PATH` comes first (as in T3 Code), then any inherited entry
//!   it lacks, so a directory the launcher added on purpose is kept.
//! * Nothing here runs on Windows: the shell already passes the owner's
//!   environment, and a PowerShell probe costs seconds on every start (T3's own
//!   reason).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one login shell may take. A profile is slow only while the machine
/// boots; past this, the inherited `PATH` is used and discovery says what it
/// cannot find.
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// A line the probe prints after `PATH`, so profile chatter on stdout cannot be
/// mistaken for the answer. `printenv` reads the environment whatever the shell's
/// own syntax is (fish's `PATH` is a list, not a colon string).
const END_MARKER: &str = "__RETINUE_PATH_END__";
const PROBE_SCRIPT: &str = "printenv PATH; echo __RETINUE_PATH_END__";

/// The shells to try: the owner's `$SHELL`, then the platform's usual default.
/// Only paths that exist are offered, in this order, without duplicates.
pub fn login_shell_candidates(shell: Option<&str>, windows: bool) -> Vec<String> {
    if windows {
        return Vec::new();
    }
    let mut candidates: Vec<String> = Vec::new();
    let mut push = |value: &str| {
        let value = value.trim();
        if !value.is_empty() && !candidates.iter().any(|seen| seen == value) {
            candidates.push(value.to_owned());
        }
    };
    if let Some(shell) = shell {
        push(shell);
    }
    for fallback in ["/bin/bash", "/bin/zsh", "/bin/sh"] {
        push(fallback);
    }
    candidates.retain(|path| std::path::Path::new(path).is_file());
    candidates
}

/// The `PATH` at the end of a login shell's stdout, or `None` when the marker is
/// missing or the line does not look like a `PATH`. `printenv` prints one line,
/// so the answer is the line before the last marker.
pub fn captured_path(output: &str) -> Option<String> {
    let mut lines = output.lines().map(str::trim).collect::<Vec<_>>();
    while let Some(line) = lines.pop() {
        if line != END_MARKER {
            continue;
        }
        let path = lines.pop()?.trim();
        return looks_like_path(path).then(|| path.to_owned());
    }
    None
}

fn looks_like_path(path: &str) -> bool {
    !path.is_empty() && !path.contains('\0') && path.contains('/') && !path.chars().any(char::is_control)
}

/// Two `PATH` values merged: the preferred (the login shell's, which knows where
/// the CLIs are) first, then the inherited one, without duplicates. An empty
/// entry is dropped; a `.` entry is kept, because it is the owner's own choice and
/// was already in the environment the daemon ran with.
pub fn merge_path(preferred: Option<&str>, inherited: Option<&str>, windows: bool) -> Option<String> {
    let separator = if windows { ';' } else { ':' };
    let mut merged: Vec<&str> = Vec::new();
    for value in [preferred, inherited].into_iter().flatten() {
        for entry in value.split(separator) {
            if entry.is_empty() || merged.contains(&entry) {
                continue;
            }
            merged.push(entry);
        }
    }
    (!merged.is_empty()).then(|| merged.join(&separator.to_string()))
}

/// Read the login shell's `PATH` and merge it into this process, once. Returns
/// `true` when the process `PATH` changed; the caller may log that in one fixed
/// sentence. A host with no usable shell, or one whose profile prints nothing
/// useful, keeps the inherited `PATH` rather than losing it.
pub fn hydrate_login_shell_path() -> bool {
    if cfg!(windows) {
        return false;
    }
    hydrate_with(
        std::env::var("SHELL").ok().as_deref(),
        std::env::var("PATH").ok(),
        probe_shell,
    )
}

/// The same step with its shell probe injected, so both outcomes are tested on
/// whichever platform runs the tests.
pub fn hydrate_with(shell: Option<&str>, inherited: Option<String>, probe: impl Fn(&str) -> Option<String>) -> bool {
    for candidate in login_shell_candidates(shell, cfg!(windows)) {
        let Some(login_path) = probe(&candidate) else { continue };
        let Some(merged) = merge_path(Some(login_path.as_str()), inherited.as_deref(), cfg!(windows)) else { continue };
        if inherited.as_deref() == Some(merged.as_str()) {
            return false;
        }
        std::env::set_var("PATH", &merged);
        return true;
    }
    false
}

/// Run one login shell's fixed probe, bounded in time and output. A profile that
/// hangs is killed; a failed start (a shell that does not exist) answers `None`.
fn probe_shell(shell: &str) -> Option<String> {
    use std::io::Read;
    let mut command = Command::new(shell);
    command
        .args(["-ilc", PROBE_SCRIPT])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0u8; 8 * 1024];
        // A profile may print without end; 64 KiB is far past any real PATH.
        let mut kept = 0usize;
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if kept < 64 * 1024 {
                        let take = read.min(64 * 1024 - kept);
                        text.push_str(&String::from_utf8_lossy(&buffer[..take]));
                        kept += take;
                    }
                }
                Err(_) => break,
            }
        }
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > SHELL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    captured_path(&reader.join().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_the_line_before_the_last_marker() {
        let output = "welcome to the machine\n__RETINUE_PATH_END__\n/home/u/.local/bin:/usr/bin:/bin\n__RETINUE_PATH_END__\n";
        assert_eq!(captured_path(output).as_deref(), Some("/home/u/.local/bin:/usr/bin:/bin"));
    }

    #[test]
    fn profile_chatter_and_missing_markers_are_not_a_path() {
        assert_eq!(captured_path("no marker here\n"), None);
        assert_eq!(captured_path("__RETINUE_PATH_END__\n"), None, "the marker with nothing before it");
        assert_eq!(captured_path("__RETINUE_PATH_END__\nnot/a/path with null\0\n__RETINUE_PATH_END__\n"), None);
        assert_eq!(captured_path("__RETINUE_PATH_END__\n\n__RETINUE_PATH_END__\n"), None, "an empty line is no path");
    }

    #[test]
    fn a_fish_style_list_is_not_mistaken_for_a_path() {
        // fish joins a quoted list with spaces, so a value with no separator at all
        // and no slash is refused rather than merged as one directory.
        assert_eq!(captured_path("__RETINUE_PATH_END__\nhome u .local bin\n__RETINUE_PATH_END__\n"), None);
    }

    #[test]
    fn merging_keeps_the_login_shell_first_and_drops_duplicates() {
        assert_eq!(
            merge_path(Some("/home/u/.local/bin:/usr/bin"), Some("/usr/bin:/bin"), false).as_deref(),
            Some("/home/u/.local/bin:/usr/bin:/bin")
        );
        assert_eq!(merge_path(Some("/a"), Some(""), false).as_deref(), Some("/a"));
        assert_eq!(merge_path(None, Some(""), false), None, "nothing to merge");
        assert!(PROBE_SCRIPT.contains(END_MARKER), "the probe prints the marker the parser reads");
    }

    #[test]
    fn candidates_are_ordered_deduplicated_and_only_real_shells() {
        let candidates = login_shell_candidates(Some("/bin/sh"), false);
        if candidates.is_empty() {
            // A test host with no /bin/sh at all (a bare Windows runner): the rule
            // is still "a path that exists".
            assert!(!std::path::Path::new("/bin/sh").is_file());
        } else {
            assert_eq!(candidates.first().map(String::as_str), Some("/bin/sh"));
            let mut seen = candidates.clone();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), candidates.len(), "no duplicates");
        }
        assert!(login_shell_candidates(Some("/bin/sh"), true).is_empty(), "Windows never probes");
    }
}
