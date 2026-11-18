//! First-run suggestions grounded in the project's own folder (wave 3, nod idea 5).
//!
//! Retinue's onboarding ends at "create the CEO", and the first thing the owner
//! sees is an empty composer. This module answers one RPC with three suggested
//! first tasks, each one read from a real file in the project the owner chose:
//! the README's first heading, a script or a crate name, the newest commits, the
//! AGENTS.md heading. When nothing can be grounded, three plain prompts stand in
//! and the reply says so (`degraded`).
//!
//! Rules, all of them the owner's:
//!
//! - Nothing runs. The reply is text; the window decides what to do when the
//!   owner sends a card.
//! - The folder is the project's own stored path, canonicalised. Only a fixed
//!   list of file names is read, each capped, each refused when it is a link or
//!   resolves outside the folder. Nothing outside that folder is read.
//! - Git is asked for the five newest subjects with an argument list and a time
//!   limit, through the one git runner the repository module already owns.
//! - Content-free logs: nothing is logged and no event is emitted. The reply
//!   names the host folder, which is why the RPC is `never` in `tiers.json`.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Most bytes read from one file.
const MAX_FILE_BYTES: u64 = 64 * 1024;
/// Longest title on a card.
const MAX_TITLE_CHARS: usize = 80;
/// Longest prompt a card sends.
const MAX_PROMPT_CHARS: usize = 500;
/// README names, in the order they are tried.
const README_NAMES: [&str; 4] = ["README.md", "README", "readme.md", "Readme.md"];
/// How many subjects `git log` is asked for.
const COMMITS: usize = 5;

/// The three suggestions when nothing at all could be read from the folder.
const STATIC: [(&str, &str, &str); 3] = [
    ("static-1", "First look", "Read this project and tell me what it does."),
    ("static-2", "Find the first fix", "Find the most useful small fix in this project, explain it, and make it if I say yes."),
    ("static-3", "Explain the build", "Explain how this project is built, tested and released."),
];

/// `project.suggestions`: three first tasks read from the project's folder.
pub(crate) async fn rpc(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let project_id = p["projectId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| derr("invalid_argument", "projectId is required", StatusCode::BAD_REQUEST))?;
    let project = st.db.project_get(project_id).map_err(|_| derr("not_found", "No such project.", StatusCode::NOT_FOUND))?;
    let stored = project["path"].as_str().map(str::trim).filter(|path| !path.is_empty()).map(str::to_owned);
    let folder = stored.as_ref().and_then(|path| readable_folder(Path::new(path)));
    let mut grounded: Vec<Value> = Vec::new();
    if let Some(folder) = folder.as_ref() {
        if let Some(suggestion) = readme_suggestion(folder) {
            grounded.push(suggestion);
        }
        if let Some(suggestion) = script_suggestion(folder) {
            grounded.push(suggestion);
        }
        if let Some(suggestion) = commit_suggestion(folder).await {
            grounded.push(suggestion);
        }
        if let Some(suggestion) = agents_suggestion(folder) {
            grounded.push(suggestion);
        }
    }
    let degraded = grounded.len() < 3;
    for (id, title, prompt) in STATIC {
        if grounded.len() >= 3 {
            break;
        }
        grounded.push(json!({"id": id, "kind": "static", "title": title, "prompt": prompt, "source": null}));
    }
    Ok(json!({
        "projectId": project_id,
        "folder": stored,
        "degraded": degraded,
        "suggestions": grounded,
    }))
}

/// The folder, when it exists, is a directory, and can be canonicalised. A
/// missing or unreadable folder is not an error: the caller falls back.
fn readable_folder(path: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    canonical.is_dir().then_some(canonical)
}

/// One file inside the folder, as text: refused when it is a link, is larger
/// than the cap, or resolves outside the folder.
fn read_inside(folder: &Path, name: &str) -> Option<String> {
    let candidate = folder.join(name);
    let meta = std::fs::symlink_metadata(&candidate).ok()?;
    if meta.file_type().is_symlink() || is_reparse(&meta) || !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let canonical = std::fs::canonicalize(&candidate).ok()?;
    if !canonical.starts_with(folder) {
        return None;
    }
    let bytes = std::fs::read(&canonical).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(windows)]
fn is_reparse(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Bounds one line of text for a card.
fn clip(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > limit {
        flat.chars().take(limit.saturating_sub(1)).collect::<String>() + "…"
    } else {
        flat
    }
}

/// The first Markdown heading in a document, without its `#` marks.
fn first_heading(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|line| line.starts_with('#')).map(|line| line.trim_start_matches('#').trim().to_owned()).filter(|heading| !heading.is_empty())
}

/// The README's first heading: "get oriented in this project".
fn readme_suggestion(folder: &Path) -> Option<Value> {
    for name in README_NAMES {
        let Some(text) = read_inside(folder, name) else { continue };
        let Some(heading) = first_heading(&text) else { continue };
        let heading = clip(&heading, 120);
        return Some(json!({
            "id": "readme",
            "kind": "readme",
            "title": clip("Get oriented", MAX_TITLE_CHARS),
            "prompt": clip(&format!("Walk me through this project: what it does, how it is laid out, and where I should start. Its README begins \"{heading}\"."), MAX_PROMPT_CHARS),
            "source": {"file": name, "detail": format!("heading: {heading}")},
        }));
    }
    None
}

/// The first `scripts` entry in `package.json`, preferring `test`, else the
/// crate name and any binary from `Cargo.toml`: "run the project's checks".
fn script_suggestion(folder: &Path) -> Option<Value> {
    if let Some(text) = read_inside(folder, "package.json") {
        if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
            if let Some(scripts) = parsed["scripts"].as_object() {
                let name = ["test", "build", "lint", "check"].iter().find(|candidate| scripts.contains_key(**candidate)).map(|name| (*name).to_owned()).or_else(|| scripts.keys().next().cloned());
                if let Some(name) = name {
                    let command = clip(&format!("npm run {name}"), 80);
                    return Some(json!({
                        "id": "script",
                        "kind": "script",
                        "title": clip("Run the checks", MAX_TITLE_CHARS),
                        "prompt": clip(&format!("Run the project's checks ({command}) and tell me what fails, then propose the smallest fix."), MAX_PROMPT_CHARS),
                        "source": {"file": "package.json", "detail": format!("script: {name}")},
                    }));
                }
            }
        }
    }
    let text = read_inside(folder, "Cargo.toml")?;
    let mut in_package = false;
    let mut package = None;
    let mut binary = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if binary.is_none() && trimmed.starts_with("name") && !in_package {
            binary = quoted_value(trimmed);
            continue;
        }
        if in_package && package.is_none() && trimmed.starts_with("name") {
            package = quoted_value(trimmed);
        }
    }
    let name = package.or(binary)?;
    Some(json!({
        "id": "script",
        "kind": "script",
        "title": clip("Run the checks", MAX_TITLE_CHARS),
        "prompt": clip(&format!("Run the project's checks (cargo test for {name}) and tell me what fails, then propose the smallest fix."), MAX_PROMPT_CHARS),
        "source": {"file": "Cargo.toml", "detail": format!("package: {name}")},
    }))
}

/// `name = "value"` on one line, without the quotes.
fn quoted_value(line: &str) -> Option<String> {
    let (_, rest) = line.split_once('=')?;
    let value = rest.trim().trim_matches('"').trim_matches('\'').trim();
    (!value.is_empty() && !value.contains(' ')).then(|| value.to_owned())
}

/// The five newest commit subjects: "pick up the last change".
async fn commit_suggestion(folder: &Path) -> Option<Value> {
    let run = crate::repository::run_git(folder, &["log", &format!("-{COMMITS}"), "--format=%h %s"], 8 * 1024).await;
    if !run.ok {
        return None;
    }
    let text = String::from_utf8_lossy(&run.stdout);
    let subjects = text.lines().map(str::trim).filter(|line| !line.is_empty()).take(COMMITS).map(|line| clip(line, 120)).collect::<Vec<_>>();
    let newest = subjects.first()?.clone();
    let listed = subjects.iter().map(|subject| format!("\"{subject}\"")).collect::<Vec<_>>().join("; ");
    Some(json!({
        "id": "commit",
        "kind": "commit",
        "title": clip("Pick up the last change", MAX_TITLE_CHARS),
        "prompt": clip(&format!("These are the newest commits here: {listed}. Read what the newest one changed and tell me what is still open or worth doing next."), MAX_PROMPT_CHARS),
        "source": {"file": Value::Null, "detail": format!("commit: {newest}")},
    }))
}

/// The AGENTS.md heading: "follow the project's rules".
fn agents_suggestion(folder: &Path) -> Option<Value> {
    let text = read_inside(folder, "AGENTS.md")?;
    let heading = first_heading(&text).unwrap_or_else(|| "its rules".to_owned());
    let heading = clip(&heading, 120);
    Some(json!({
        "id": "agents",
        "kind": "agents",
        "title": clip("Follow the project's rules", MAX_TITLE_CHARS),
        "prompt": clip(&format!("Read this project's AGENTS.md (\"{heading}\") and tell me the rules I must not break while working here."), MAX_PROMPT_CHARS),
        "source": {"file": "AGENTS.md", "detail": format!("heading: {heading}")},
    }))
}
