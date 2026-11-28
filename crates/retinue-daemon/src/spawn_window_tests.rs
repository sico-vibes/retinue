//! Gate: no process Retinue starts shows a console window on Windows.
//!
//! The window has one cause: a console program started by a process that has no
//! console of its own gets one (`docs/history/fix-35/c5-windows.md:9`). The daemon
//! and the native shell are built without a console, so every child they start
//! must say so. `retinue_process::prepare_command` (job-owned: paused until the
//! tree attaches) and `hide_window`/`hide_std_window` (not job-owned) are the two
//! ways; a site may also write the flag itself.
//!
//! This scans every `Command::new(` in the crates and the shell, the way
//! `fix39_p12_tests::every_log_call_site_is_a_fixed_message` scans every log call
//! site, because Windows has no getter for creation flags and no test can watch
//! for a window. `docs/reference/invariants.md`, "No process Retinue starts shows
//! a console window on Windows" (21 November 2026).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The spawn call the scan looks for, split so this file does not contain it.
fn spawn_needle() -> String {
    ["Command", "::new("].concat()
}

/// Anything in the text between the spawn and the call that runs it that makes
/// the child windowless.
const HIDING: &[&str] = &[
    "hide_window(",
    "hide_std_window(",
    "prepare_command(",
    "prepare_std_command(",
    "CREATE_NO_WINDOW",
    "0x0800_0000",
    "0x08000000",
    "0x0800_0004",
    "0x08000004",
];

/// Whole files the scan does not read, with the reason. They are test code or
/// tools a person runs from a console, never the GUI shell.
const SKIPPED: &[(&str, &str)] = &[
    ("_tests.rs", "a test module: the test runner is a console program, so a child inherits its console and gets no window of its own"),
    ("/src/bin/", "a developer or fixture binary run by hand or by the test runner from a console, not part of the app"),
];

const TEST_HELPER: &str = "a test helper: the test runner is a console program, so this child inherits its console and gets no window of its own; it never runs from the GUI shell";

/// A spawn that needs no flag, or must not have one. Matching is by file and the
/// spawn line, and the count is exact: a new spawn in the same file fails this
/// gate until it is hidden or added here on purpose.
struct Allowed {
    path: &'static str,
    needle: &'static str,
    count: usize,
    reason: &'static str,
}

const ALLOWED: &[Allowed] = &[
    Allowed { path: "crates/retinue-adapter-acp/src/catalog.rs", needle: r#"Command::new("fake-opencode")"#, count: 1, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-adapter-acp/src/exec.rs", needle: r#"Command::new("fake-opencode")"#, count: 1, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/experiments.rs", needle: r#"Command::new("git")"#, count: 2, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/main.rs", needle: r#"Command::new("git")"#, count: 4, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/worktree.rs", needle: r#"Command::new("git")"#, count: 2, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/worktree.rs", needle: r#"Command::new("ping")"#, count: 1, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/worktree_cleanup.rs", needle: r#"Command::new("git")"#, count: 4, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/worktree_cleanup.rs", needle: r#"Command::new("cmd")"#, count: 2, reason: TEST_HELPER },
    Allowed { path: "crates/retinue-daemon/src/media.rs", needle: r#"Command::new("xdg-open")"#, count: 1, reason: "Linux's file manager: it runs where there is no console window to show (Windows uses explorer.exe above, which is flagged)" },
    Allowed { path: "crates/retinue-daemon/src/connectors_oauth.rs", needle: r#"Command::new("xdg-open")"#, count: 1, reason: "Linux's browser opener for a connector sign-in: it runs where there is no console window to show (Windows uses ShellExecuteW above)" },
    Allowed { path: "crates/retinue-daemon/src/project_clone.rs", needle: r#"Command::new("kill")"#, count: 1, reason: "the Unix half of the stop path: it runs where there is no console window to show" },
    Allowed { path: "crates/retinue-runtime/src/shell.rs", needle: "Command::new(shell)", count: 1, reason: "the login shell probe is Unix-only (`hydrate_login_shell_path` returns at once on Windows): there is no console window to hide on the platform that runs it" },
    Allowed { path: "apps/desktop/src-tauri/src/lib.rs", needle: "Command::new(executable)", count: 1, reason: "the owner's own editor: a visible GUI program, and hiding it would hide the editor they chose" },
    Allowed { path: "apps/desktop/src-tauri/src/lib.rs", needle: r#"Command::new("explorer.exe")"#, count: 1, reason: "File Explorer is a GUI program and has no console of its own" },
    Allowed { path: "apps/desktop/src-tauri/src/lib.rs", needle: "Command::new(&launch.program)", count: 1, reason: "the owner's sign-in terminal, visible by design: the owner types into it (CREATE_NEW_CONSOLE at the same site)" },
];

#[test]
fn no_spawn_that_can_show_a_console_window() {
    let root = Path::new("../..");
    assert!(root.join("crates").is_dir(), "the scan must run from crates/retinue-daemon, or it would check nothing");
    let mut roots: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
        .expect("the repository has a crates folder")
        .flatten()
        .map(|entry| entry.path().join("src"))
        .filter(|dir| dir.is_dir())
        .collect();
    roots.push(root.join("apps/desktop/src-tauri/src"));
    assert!(roots.len() > 10, "every crate's sources are scanned, not a handful: {}", roots.len());

    let needle = spawn_needle();
    let mut files_scanned = 0usize;
    let mut spawns = 0usize;
    let mut hidden = 0usize;
    let mut matched: HashMap<usize, usize> = HashMap::new();
    let mut failures: Vec<String> = Vec::new();

    for dir in &roots {
        let files = source_files(dir);
        assert!(!files.is_empty(), "no Rust sources were found under {}: the scan would check nothing", dir.display());
        for file in files {
            let path = relative(root, &file);
            if SKIPPED.iter().any(|(marker, _)| path.contains(marker)) {
                continue;
            }
            files_scanned += 1;
            let source = without_comments(&std::fs::read_to_string(&file).unwrap_or_default());
            let mut from = 0;
            while let Some(found) = source[from..].find(&needle) {
                let at = from + found;
                spawns += 1;
                if window_hides(&source[at..]) {
                    hidden += 1;
                } else {
                    let line = trimmed_line(&source, at);
                    match ALLOWED.iter().enumerate().find(|(_, entry)| path == entry.path && line.contains(entry.needle)) {
                        Some((index, _)) => *matched.entry(index).or_default() += 1,
                        None => failures.push(format!(
                            "{path}:{}: a spawn sets no no-window flag and is not an allowed exception: {line}",
                            line_number(&source, at)
                        )),
                    }
                }
                from = at + needle.len();
            }
        }
    }

    for (index, entry) in ALLOWED.iter().enumerate() {
        let seen = matched.get(&index).copied().unwrap_or(0);
        if seen != entry.count {
            failures.push(format!(
                "{}: the exception for {:?} matches {seen} spawn(s) and says {}; update the count, or the entry if the spawn is gone (reason it exists: {})",
                entry.path, entry.needle, entry.count, entry.reason
            ));
        }
    }

    assert!(files_scanned > 50, "too few files were scanned ({files_scanned}): a moved folder would make this pass while checking nothing");
    assert!(spawns > 0, "no spawns were found at all: the scan would check nothing");
    assert!(hidden > 0, "no spawn was recognised as hidden: the hiding tokens match nothing");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The text from the spawn to the call that runs the child (or a bounded window
/// when there is none, as for a helper that returns a `Command`).
fn window_hides(rest: &str) -> bool {
    // Offsets come from `find`, so they sit on character boundaries; the 4000-byte
    // bound does not, and a non-ASCII string literal before the spawn would panic
    // the slice rather than fail the gate.
    let mut end = rest.len().min(4000);
    while end > 0 && !rest.is_char_boundary(end) {
        end -= 1;
    }
    for stop in [".spawn(", ".output(", ".status("] {
        if let Some(found) = rest.find(stop) {
            end = end.min(found);
        }
    }
    let window = &rest[..end];
    HIDING.iter().any(|token| window.contains(token))
}

fn trimmed_line(source: &str, at: usize) -> String {
    let start = source[..at].rfind('\n').map(|index| index + 1).unwrap_or(0);
    let end = source[at..].find('\n').map(|offset| at + offset).unwrap_or(source.len());
    source[start..end].trim().to_owned()
}

fn line_number(source: &str, at: usize) -> usize {
    source[..at].matches('\n').count() + 1
}

fn relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/")
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(source_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    out
}

/// The source with every comment replaced by spaces, so a comment that mentions
/// a flag cannot make a spawn pass. Offsets and line numbers are unchanged.
/// String, byte-string and char literals and raw strings are left as they are.
fn without_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index += 2;
                        continue;
                    }
                    if bytes[index] == b'"' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
            }
            b'\'' => {
                let mut close = index + 1;
                if bytes.get(close) == Some(&b'\\') {
                    close += 2;
                } else {
                    close += 1;
                }
                // `'a'` closes; `'a` is a lifetime.
                index = if bytes.get(close) == Some(&b'\'') { close + 1 } else { index + 1 };
            }
            b'r' => {
                let mut quote = index + 1;
                let mut hashes = 0;
                while bytes.get(quote) == Some(&b'#') {
                    hashes += 1;
                    quote += 1;
                }
                if bytes.get(quote) == Some(&b'"') {
                    let mut close = vec![b'"'];
                    close.extend(std::iter::repeat_n(b'#', hashes));
                    index = find_bytes(&bytes[quote + 1..], &close)
                        .map(|offset| quote + 1 + offset + close.len())
                        .unwrap_or(bytes.len());
                } else {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                let mut end = index;
                while end < bytes.len() && bytes[end] != b'\n' {
                    end += 1;
                }
                out[index..end].fill(b' ');
                index = end;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let mut depth = 1;
                let mut end = index + 2;
                while end < bytes.len() && depth > 0 {
                    if bytes[end] == b'/' && bytes.get(end + 1) == Some(&b'*') {
                        depth += 1;
                        end += 2;
                        continue;
                    }
                    if bytes[end] == b'*' && bytes.get(end + 1) == Some(&b'/') {
                        depth -= 1;
                        end += 2;
                        continue;
                    }
                    end += 1;
                }
                out[index..end].fill(b' ');
                index = end;
            }
            _ => index += 1,
        }
    }
    String::from_utf8(out).expect("replacing comments with spaces keeps UTF-8")
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[test]
fn a_commented_flag_is_not_a_hidden_spawn() {
    let source = "// prepare_command(&mut command); CREATE_NO_WINDOW\nlet mut c = Command::new(\"x\");\nc.creation_flags(0x0800_0000);\n";
    let stripped = without_comments(source);
    assert_eq!(stripped.lines().count(), source.lines().count(), "line numbers survive");
    let spawn = stripped.find("Command::new(").expect("the spawn survives");
    assert!(window_hides(&stripped[spawn..]), "the real flag after the spawn still counts");
    let commented = "/* hide_window(&mut c); */ let mut c = Command::new(\"x\");\n";
    let stripped = without_comments(commented);
    assert!(!window_hides(&stripped[stripped.find("Command::new(").unwrap()..]), "a commented helper is not a helper");
    assert!(without_comments("let s = \"// not a comment\";").contains("// not a comment"), "a string is not a comment");
}
