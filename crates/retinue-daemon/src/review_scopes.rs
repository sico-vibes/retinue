//! Review scopes: which range of the repository the review screen shows.
//!
//! `session.repository` and `session.git.diff` take an optional `scope`:
//!
//! - `uncommitted`, and an absent `scope`: the working tree against `HEAD`, answered by
//!   `repository::working_tree` and `git_ops::uncommitted` exactly as before.
//! - `turn` with `turnId`: the files one turn changed, from the checkpoints the conversation
//!   already records. A recorded diff is shown only while Git still applies it in reverse, the
//!   same rule the checkpoints report uses, so a file edited since that turn is listed with a
//!   plain reason and no diff rather than a wrong one.
//! - `branch`: the conversation's branch against the point it left its base branch at (their
//!   merge base), uncommitted work included. The base is the branch this task copy was created
//!   from when the conversation records one, otherwise the repository's default branch:
//!   `origin/HEAD`, then a local `main`, then a local `master`. The answer says which, in
//!   `base.how`.
//! - `commit`: the last commit against its parent. A root commit is compared against nothing; a
//!   merge has no single diff and says so.
//!
//! Every scope keeps the rules of `repository`: read-only, argument lists (never a shell), no
//! external diff driver or textconv, bounded time and bytes, the folder taken from the
//! conversation row, and nothing logged.

use crate::git_ops::{DIFF_MAX_BYTES, NUMSTAT_MAX_BYTES as WHOLE_NUMSTAT_MAX_BYTES};
use crate::repository::{
    self, absolute_path, display_path, empty_answer, locate, parse_numstat, patch_of, repo_answer, run_git, text,
    untracked_file, Change, Located, MAX_FILES, MAX_PATCH_FILES, NUMSTAT_MAX_BYTES, PATCH_POOL_BYTES,
    TOPLEVEL_MAX_BYTES,
};
use crate::{checkpoints, derr, experiments, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// No base branch could be found, so `branch` has nothing to compare against.
const NO_BASE: &str = "This repository has no default branch to compare against, so there is nothing to show for the branch.";
const NO_HISTORY: &str = "This branch and the base branch have no shared history, so there is nothing to show for the branch.";
const NO_TURN: &str = "No changes were recorded for that turn.";

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

// ── which scope ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Uncommitted,
    Turn(String),
    Branch,
    Commit,
}

impl Scope {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Scope::Uncommitted => "uncommitted",
            Scope::Turn(_) => "turn",
            Scope::Branch => "branch",
            Scope::Commit => "commit",
        }
    }
}

/// The scope one call asks for; an absent, `null` or empty `scope` is `uncommitted`.
pub(crate) fn parse(p: &Value) -> Result<Scope, DispatchError> {
    let requested = match p.get("scope") {
        None | Some(Value::Null) => return Ok(Scope::Uncommitted),
        Some(Value::String(text)) => text.trim(),
        Some(_) => return Err(invalid("scope must be uncommitted, turn, branch or commit.")),
    };
    if requested.is_empty() { return Ok(Scope::Uncommitted); }
    match requested {
        "uncommitted" => Ok(Scope::Uncommitted),
        "turn" => {
            let turn_id = p["turnId"].as_str().map(str::trim).filter(|id| !id.is_empty() && id.len() <= 200)
                .ok_or_else(|| invalid("turnId is required for the turn scope."))?;
            Ok(Scope::Turn(turn_id.to_owned()))
        }
        "branch" => Ok(Scope::Branch),
        "commit" => Ok(Scope::Commit),
        _ => Err(invalid("scope must be uncommitted, turn, branch or commit.")),
    }
}

/// Adds which scope was applied, and for `branch` what it compared against, to an answer.
pub(crate) fn stamp(mut answer: Value, scope: &Scope, base: Option<&Base>) -> Value {
    if let Some(fields) = answer.as_object_mut() {
        fields.insert("scope".to_owned(), json!(scope.name()));
        fields.insert("base".to_owned(), base.map(Base::json).unwrap_or(Value::Null));
    }
    answer
}

/// The two ends of a scope's comparison, as `git` takes them.
#[derive(Clone, Debug)]
pub(crate) enum Ends {
    /// One commit against the working tree: `git diff <from>`.
    Worktree(String),
    /// The last commit against its parent: `git diff-tree --root`, which also covers a root
    /// commit and prints nothing for a merge.
    LastCommit,
}

impl Ends {
    /// A whole argument list for one git call: the ends first, then the options, then `--` and
    /// the paths when there are any.
    pub(crate) fn args(&self, options: &[&str], paths: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = match self {
            Ends::Worktree(from) => vec!["diff".to_owned(), from.clone()],
            Ends::LastCommit => vec!["diff-tree".to_owned(), "--root".to_owned(), "-r".to_owned(), "--no-commit-id".to_owned(), "HEAD".to_owned()],
        };
        args.extend(options.iter().map(|option| (*option).to_owned()));
        if !paths.is_empty() {
            args.push("--".to_owned());
            args.extend(paths.iter().map(|path| (*path).to_owned()));
        }
        args
    }
}

/// One git call from built arguments, so `Ends` can stay a plain value.
async fn git(root: &Path, args: Vec<String>, max_bytes: usize) -> repository::GitRun {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git(root, &refs, max_bytes).await
}

/// A path as a Git pathspec that is only ever itself.
fn literal(path: &str) -> String {
    format!(":(literal){path}")
}

// ── the base a branch is compared against ────────────────────────────────

/// What `branch` compares against, and how that base was decided.
pub(crate) struct Base {
    reference: String,
    commit: String,
    kind: &'static str,
    how: &'static str,
}

impl Base {
    fn json(&self) -> Value {
        json!({"ref": self.reference, "commit": self.commit, "kind": self.kind, "how": self.how})
    }
}

/// The commit `name` resolves to, or `None` when it does not name one.
async fn commit_of(root: &Path, name: &str) -> Option<String> {
    let run = git(root, vec!["rev-parse".to_owned(), "--verify".to_owned(), "-q".to_owned(), format!("{name}^{{commit}}")], TOPLEVEL_MAX_BYTES).await;
    let sha = text(&run.stdout).trim().to_owned();
    (run.ok && !sha.is_empty()).then_some(sha)
}

/// `origin/HEAD`, when the remote set one and it resolves to a commit here.
async fn origin_head(root: &Path) -> Option<String> {
    let run = git(root, vec!["symbolic-ref".to_owned(), "--short".to_owned(), "-q".to_owned(), "refs/remotes/origin/HEAD".to_owned()], TOPLEVEL_MAX_BYTES).await;
    let name = text(&run.stdout).trim().to_owned();
    if !run.ok || name.is_empty() { return None; }
    commit_of(root, &name).await.map(|_| name)
}

/// The merge base of `reference` and `HEAD`, which is the point this branch left it.
async fn merge_base(root: &Path, reference: &str, kind: &'static str, how: &'static str) -> Result<Base, &'static str> {
    let run = git(root, vec!["merge-base".to_owned(), reference.to_owned(), "HEAD".to_owned()], TOPLEVEL_MAX_BYTES).await;
    let commit = text(&run.stdout).trim().to_owned();
    if !run.ok || commit.is_empty() { return Err(NO_HISTORY); }
    Ok(Base { reference: reference.to_owned(), commit, kind, how })
}

/// The base branch the `branch` scope compares against: the branch this task copy was created
/// from when the conversation records one, otherwise the repository's default branch.
async fn base_of(detail: &Value, root: &Path) -> Result<Base, &'static str> {
    let recorded = detail["link"]["worktree"]["base"].as_str().map(str::trim).filter(|name| !name.is_empty());
    if let Some(name) = recorded {
        if commit_of(root, name).await.is_some() {
            return merge_base(root, name, "worktreeBase", "The branch this task copy was created from.").await;
        }
    }
    if let Some(name) = origin_head(root).await {
        return merge_base(root, &name, "originHead", "The repository's default branch, from origin/HEAD.").await;
    }
    for (name, how) in [("main", "The repository's default branch, main."), ("master", "The repository's default branch, master.")] {
        if commit_of(root, name).await.is_some() {
            return merge_base(root, name, "localDefault", how).await;
        }
    }
    Err(NO_BASE)
}

/// How many parents the last commit has; more than one is a merge.
async fn commit_parents(root: &Path) -> usize {
    let run = git(root, vec!["rev-list".to_owned(), "--parents".to_owned(), "-n".to_owned(), "1".to_owned(), "HEAD".to_owned()], TOPLEVEL_MAX_BYTES).await;
    text(&run.stdout).split_whitespace().count().saturating_sub(1)
}

// ── the file list for a scope ────────────────────────────────────────────

/// One scope's list of changed files and their totals.
struct Listing {
    files: Vec<Value>,
    added: u64,
    removed: u64,
    truncated: bool,
}

/// `git diff --name-status -z` between a scope's two ends. With `-z` each record is the status,
/// a NUL, then the path (and for a rename or copy the old path, a NUL, then the new one).
pub(crate) fn parse_name_status(stdout: &[u8]) -> Vec<Change> {
    let raw = text(stdout);
    let mut fields = raw.split('\0');
    let mut changes = Vec::new();
    while let Some(status) = fields.next() {
        let Some(letter) = status.chars().next() else { continue };
        if status.is_empty() { continue; }
        let Some(path) = fields.next() else { break };
        if path.is_empty() { break; }
        if matches!(letter, 'R' | 'C') {
            let Some(new_path) = fields.next() else { break };
            if new_path.is_empty() { break; }
            changes.push(Change {
                repository_path: new_path.to_owned(),
                previous_path: Some(path.to_owned()),
                change: if letter == 'R' { "renamed" } else { "added" },
                untracked: false,
                staged: "none",
                special: false,
            });
        } else {
            changes.push(Change {
                repository_path: path.to_owned(),
                previous_path: None,
                change: match letter { 'A' => "added", 'D' => "deleted", _ => "modified" },
                untracked: false,
                staged: "none",
                special: false,
            });
        }
    }
    changes
}

async fn entries(root: &Path, ends: &Ends) -> Result<Vec<Change>, &'static str> {
    let run = git(root, ends.args(&["-M", "--name-status", "-z", "--no-ext-diff", "--no-textconv"], &[]), NUMSTAT_MAX_BYTES).await;
    if !run.started { return Err("Git is not available on this computer."); }
    if run.truncated || (!run.ok && run.stdout.is_empty()) { return Err("The changes for this scope could not be read."); }
    Ok(parse_name_status(&run.stdout))
}

/// One file's whole patch between a scope's two ends and its line counts, with the same limits
/// and answers `session.git.diff` uses against `HEAD`.
async fn whole(root: &Path, ends: &Ends, change: &Change) -> crate::git_ops::Whole {
    let mut specs = vec![literal(&change.repository_path)];
    if let Some(previous) = &change.previous_path { specs.push(literal(previous)); }
    let spec_refs: Vec<&str> = specs.iter().map(String::as_str).collect();
    let numstat = git(root, ends.args(&["-M", "--numstat", "-z", "--no-ext-diff", "--no-textconv"], &spec_refs), WHOLE_NUMSTAT_MAX_BYTES).await;
    let (mut added, mut removed, mut binary, mut counted) = (0u64, 0u64, false, false);
    if numstat.ok {
        for stat in parse_numstat(&numstat.stdout).values() {
            counted = true;
            binary |= stat.binary;
            added += stat.added.unwrap_or(0);
            removed += stat.removed.unwrap_or(0);
        }
    }
    if binary { return (None, false, true, None, None); }
    let run = git(root, ends.args(&["-p", "-M", "--no-ext-diff", "--no-textconv"], &spec_refs), DIFF_MAX_BYTES).await;
    if !run.started || (!run.ok && run.stdout.is_empty() && !run.truncated) { return (None, false, false, None, None); }
    let out = text(&run.stdout);
    let diff = (!out.trim().is_empty()).then_some(out);
    (diff, run.truncated, false, counted.then_some(added), counted.then_some(removed))
}

/// The changed files between a scope's two ends, with the same caps and the same inline diffs
/// `session.repository` builds against `HEAD`. `status` supplies the untracked files
/// (`untracked`) and how much of each file's current change is staged, and is empty for
/// `commit`.
async fn ranged_listing(root: &Path, ends: &Ends, status: &[Change], untracked: bool) -> Result<Listing, &'static str> {
    let mut changes = entries(root, ends).await?;
    if untracked {
        changes.extend(status.iter().filter(|change| change.untracked).map(|change| Change {
            repository_path: change.repository_path.clone(),
            previous_path: None,
            change: "added",
            untracked: true,
            staged: "none",
            special: false,
        }));
    }
    let mut stats = HashMap::new();
    let mut stats_truncated = false;
    let numstat = git(root, ends.args(&["-M", "--numstat", "-z", "--no-ext-diff", "--no-textconv"], &[]), NUMSTAT_MAX_BYTES).await;
    if numstat.ok {
        stats = parse_numstat(&numstat.stdout);
        stats_truncated = numstat.truncated;
    }
    let mut files = Vec::new();
    let mut pool = PATCH_POOL_BYTES;
    let (mut total_added, mut total_removed) = (0u64, 0u64);
    let mut truncated = stats_truncated || changes.len() > MAX_FILES;
    let mut patches = 0usize;
    for change in changes.iter().take(MAX_FILES) {
        let mut added: Option<u64> = None;
        let mut removed: Option<u64> = None;
        let mut binary = false;
        let mut built: Option<(Option<String>, bool)> = None;
        if change.untracked {
            let read = untracked_file(root, &change.repository_path, &mut pool);
            added = read.added;
            removed = read.removed;
            binary = read.binary;
            built = Some((read.diff, read.truncated));
        } else {
            let mut found = false;
            let (mut sum_added, mut sum_removed) = (0u64, 0u64);
            for key in [Some(change.repository_path.as_str()), change.previous_path.as_deref()].into_iter().flatten() {
                if let Some(stat) = stats.remove(key) {
                    found = true;
                    binary |= stat.binary;
                    sum_added += stat.added.unwrap_or(0);
                    sum_removed += stat.removed.unwrap_or(0);
                }
            }
            if found && !binary { added = Some(sum_added); removed = Some(sum_removed); }
            // A pure rename has no content of its own to show.
            let pure_rename = change.change == "renamed" && found && sum_added == 0 && sum_removed == 0;
            if !binary && !pure_rename {
                if patches >= MAX_PATCH_FILES || pool == 0 {
                    truncated = true;
                } else {
                    patches += 1;
                    let mut specs = vec![literal(&change.repository_path)];
                    if let Some(previous) = &change.previous_path { specs.push(literal(previous)); }
                    let spec_refs: Vec<&str> = specs.iter().map(String::as_str).collect();
                    let patch = patch_of(root, ends.args(&["-p", "-M", "--no-ext-diff", "--no-textconv"], &spec_refs), &mut pool).await;
                    truncated |= patch.truncated;
                    built = Some((patch.diff, patch.truncated));
                }
            }
        }
        let (diff, file_truncated) = built.unwrap_or((None, false));
        truncated |= file_truncated;
        total_added += added.unwrap_or(0);
        total_removed += removed.unwrap_or(0);
        let staged = status.iter().find(|entry| !entry.untracked && entry.repository_path == change.repository_path).map(|entry| entry.staged).unwrap_or("none");
        files.push(json!({
            "path": absolute_path(root, &change.repository_path).map(|path| display_path(&path)).unwrap_or_else(|| display_path(root)),
            "repositoryPath": change.repository_path,
            "change": change.change,
            "addedLines": added,
            "removedLines": removed,
            "diff": diff,
            "binary": binary,
            "truncated": file_truncated,
            "staged": staged,
        }));
    }
    Ok(Listing { files, added: total_added, removed: total_removed, truncated })
}

// ── the turn scope, from what the conversation recorded ──────────────────

/// The recorded path of one file change; providers report it relative to the project or
/// absolute.
fn recorded_path_of(file: &Value) -> &str {
    file["detail"]["path"].as_str().filter(|path| !path.is_empty()).or_else(|| file["path"].as_str()).unwrap_or("")
}

/// The recorded files of one turn, oldest first.
fn recorded_files(detail: &Value, turn_id: &str) -> Vec<Value> {
    detail["files"].as_array().cloned().unwrap_or_default().into_iter().filter(|file| file["turnId"].as_str() == Some(turn_id)).collect()
}

/// A displayed path without `..`, drive or empty parts, or `None` when it leaves the
/// repository.
fn clean_path(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." { continue; }
        if part == ".." || part.contains(':') { return None; }
        parts.push(part);
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `text` without the `root` prefix, when it starts with it (case-insensitive, either
/// separator).
fn strip_root<'a>(text: &'a str, root: &str) -> Option<&'a str> {
    (text.len() > root.len() && text.is_char_boundary(root.len()) && text[..root.len()].eq_ignore_ascii_case(root))
        .then(|| text[root.len()..].trim_start_matches('/'))
}

/// A path a provider recorded, as a repository path. An absolute path loses the repository
/// root; anything else is relative to the project folder, which may be a repository subfolder.
/// A path outside the repository, or one that is not safe, is `None`.
pub(crate) fn repository_relative(project: &Path, root: &Path, recorded: &str) -> Option<String> {
    let root_text = display_path(root).replace('\\', "/");
    let root_trim = root_text.trim_end_matches('/');
    let text = recorded.replace('\\', "/");
    if let Some(rest) = strip_root(&text, root_trim) { return clean_path(rest); }
    let project_text = display_path(project).replace('\\', "/");
    match strip_root(&project_text, root_trim).and_then(clean_path) {
        Some(prefix) => clean_path(&format!("{prefix}/{text}")),
        None => clean_path(&text),
    }
}

/// One recorded change as the review screen reads it: a diff only while the file still matches
/// what the turn left, otherwise the state and a plain reason.
fn turn_entry(project: &Path, change: &Value, recorded_path: &str, outside: bool) -> (Option<String>, &'static str, Option<&'static str>, bool) {
    if outside || recorded_path.is_empty() {
        return (None, "blocked", Some("That file is outside this repository, so its change can't be shown."), true);
    }
    if change["reverted"] == true {
        return (None, "reverted", Some("This change was already undone."), true);
    }
    let outcome = experiments::reverse_file_check(project, recorded_path, change);
    let complete = checkpoints::diff_complete(&outcome);
    match outcome {
        Ok(()) => (change["diff"].as_str().filter(|text| !text.is_empty()).map(str::to_owned), "available", None, true),
        Err(_) if !complete => (None, "blocked", Some("No complete diff was saved for this change, so it can't be shown."), false),
        Err(_) => (None, "blocked", Some("This file has changed since that turn, so its change from that turn can't be shown."), true),
    }
}

/// The provider's kind as the four words the review screen knows.
fn change_word(kind: Option<&str>) -> &'static str {
    match kind.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "create" | "created" | "add" | "added" | "new" => "added",
        "delete" | "deleted" | "remove" | "removed" => "deleted",
        "rename" | "renamed" | "move" | "moved" => "renamed",
        _ => "modified",
    }
}

fn turn_listing(project: &Path, root: &Path, detail: &Value, turn_id: &str) -> Result<Listing, &'static str> {
    let recorded = recorded_files(detail, turn_id);
    if recorded.is_empty() { return Err(NO_TURN); }
    let mut files = Vec::new();
    let mut pool = PATCH_POOL_BYTES;
    let mut truncated = recorded.len() > MAX_FILES;
    let (mut total_added, mut total_removed) = (0u64, 0u64);
    for file in recorded.iter().take(MAX_FILES) {
        let change = &file["detail"];
        let recorded_path = recorded_path_of(file);
        let relative = repository_relative(project, root, recorded_path);
        let (mut diff, state, reason, complete) = turn_entry(project, change, recorded_path, relative.is_none());
        if let Some(text) = diff.clone() {
            if text.len() > pool { diff = None; truncated = true; } else { pool -= text.len(); }
        }
        let (additions, deletions) = checkpoints::line_counts(change["diff"].as_str());
        total_added += u64::from(additions.unwrap_or(0));
        total_removed += u64::from(deletions.unwrap_or(0));
        let display = relative.as_deref().and_then(|path| absolute_path(root, path)).map(|path| display_path(&path)).unwrap_or_else(|| recorded_path.to_owned());
        files.push(json!({
            "path": display,
            "repositoryPath": relative.unwrap_or_else(|| recorded_path.to_owned()),
            "change": change_word(change["change"].as_str()),
            "addedLines": additions,
            "removedLines": deletions,
            "diff": diff,
            "binary": false,
            "truncated": false,
            "staged": "none",
            "state": state,
            "reason": reason,
            "diffComplete": complete,
        }));
    }
    Ok(Listing { files, added: total_added, removed: total_removed, truncated })
}

// ── the answers ──────────────────────────────────────────────────────────

/// `session.repository` for a scope other than `uncommitted`.
pub(crate) async fn tree(project_path: &str, detail: &Value, scope: &Scope) -> Value {
    if matches!(scope, Scope::Uncommitted) { return repository::working_tree(project_path).await; }
    let folder = PathBuf::from(project_path);
    if !folder.is_dir() { return stamp(empty_answer(Some(project_path.to_owned()), "The project folder is missing."), scope, None); }
    let located = match locate(project_path).await {
        Ok(located) => located,
        Err(message) => return stamp(empty_answer(Some(project_path.to_owned()), message), scope, None),
    };
    let (branch, detached) = repository::current_branch(&located.root).await;
    let mut base: Option<Base> = None;
    let listing = match scope {
        Scope::Turn(turn_id) => turn_listing(&folder, &located.root, detail, turn_id),
        Scope::Branch => match base_of(detail, &located.root).await {
            Ok(found) => {
                let ends = Ends::Worktree(found.commit.clone());
                base = Some(found);
                ranged_listing(&located.root, &ends, &located.changes, true).await
            }
            Err(message) => Err(message),
        },
        Scope::Commit => {
            if located.unborn { Err("This repository has no commits yet.") }
            else if commit_parents(&located.root).await > 1 { Err("The last commit merges other branches, so there is no single diff to show.") }
            else { ranged_listing(&located.root, &Ends::LastCommit, &[], false).await }
        }
        // Answered at the top of this function; kept here so the match is exhaustive.
        Scope::Uncommitted => return repository::working_tree(project_path).await,
    };
    let answer = match listing {
        Ok(listing) => repo_answer(project_path, &located.root, branch, detached, listing.files, (listing.added, listing.removed), listing.truncated, None),
        Err(message) => repo_answer(project_path, &located.root, branch, detached, Vec::new(), (0, 0), false, Some(message)),
    };
    stamp(answer, scope, base.as_ref())
}

/// The answer for a path that has no change in this scope.
fn nothing(path: &str, reason: Option<&str>) -> Value {
    json!({
        "repositoryPath": path,
        "previousPath": Value::Null,
        "change": Value::Null,
        "diff": Value::Null,
        "truncated": false,
        "binary": false,
        "addedLines": Value::Null,
        "removedLines": Value::Null,
        "staged": "none",
        "reason": reason,
    })
}

/// One file's whole change between a scope's two ends, in the same shape as the answer against
/// `HEAD`.
async fn ranged_file(root: &Path, ends: &Ends, status: &[Change], path: &str, untracked: bool) -> Value {
    if untracked {
        if status.iter().any(|change| change.untracked && change.repository_path == path) {
            let (diff, truncated, binary, added, removed) = crate::git_ops::synthesise(root, path);
            return json!({
                "repositoryPath": path,
                "previousPath": Value::Null,
                "change": "added",
                "diff": diff,
                "truncated": truncated,
                "binary": binary,
                "addedLines": added,
                "removedLines": removed,
                "staged": "none",
                "reason": Value::Null,
            });
        }
    }
    let changes = match entries(root, ends).await {
        Ok(changes) => changes,
        Err(message) => return nothing(path, Some(message)),
    };
    let found = changes.iter().find(|change| change.repository_path == path)
        .or_else(|| changes.iter().find(|change| change.previous_path.as_deref() == Some(path)));
    let Some(change) = found else { return nothing(path, None) };
    let (diff, truncated, binary, added, removed) = whole(root, ends, change).await;
    json!({
        "repositoryPath": change.repository_path,
        "previousPath": change.previous_path,
        "change": change.change,
        "diff": diff,
        "truncated": truncated,
        "binary": binary,
        "addedLines": added,
        "removedLines": removed,
        "staged": "none",
        "reason": Value::Null,
    })
}

/// One recorded change of a turn, in the same shape as the answer against `HEAD`, with the
/// state and the reason a diff cannot be shown.
fn turn_file(project: &Path, root: &Path, detail: &Value, turn_id: &str, path: &str) -> Value {
    let recorded = recorded_files(detail, turn_id);
    let found = recorded.iter().find(|file| repository_relative(project, root, recorded_path_of(file)).as_deref() == Some(path))
        .or_else(|| recorded.iter().find(|file| recorded_path_of(file) == path));
    let Some(file) = found else { return nothing(path, Some("This file is not part of that turn.")) };
    let change = &file["detail"];
    let recorded_path = recorded_path_of(file);
    let relative = repository_relative(project, root, recorded_path);
    let (diff, state, reason, complete) = turn_entry(project, change, recorded_path, relative.is_none());
    let (additions, deletions) = checkpoints::line_counts(change["diff"].as_str());
    json!({
        "repositoryPath": relative.unwrap_or_else(|| recorded_path.to_owned()),
        "previousPath": Value::Null,
        "change": change_word(change["change"].as_str()),
        "diff": diff,
        "truncated": false,
        "binary": false,
        "addedLines": additions,
        "removedLines": deletions,
        "staged": "none",
        "state": state,
        "reason": reason,
        "diffComplete": complete,
    })
}

/// `session.git.diff` for a scope other than `uncommitted`. The folder is the conversation's
/// own, and a folder that is not a Git repository is refused by the caller, as it always was.
pub(crate) async fn file_diff(project: &str, detail: &Value, located: &Located, scope: &Scope, path: &str) -> Result<Value, DispatchError> {
    let folder = PathBuf::from(project);
    match scope {
        Scope::Turn(turn_id) => Ok(stamp(turn_file(&folder, &located.root, detail, turn_id, path), scope, None)),
        Scope::Branch => match base_of(detail, &located.root).await {
            Ok(base) => {
                let answer = ranged_file(&located.root, &Ends::Worktree(base.commit.clone()), &located.changes, path, true).await;
                Ok(stamp(answer, scope, Some(&base)))
            }
            Err(message) => Ok(stamp(nothing(path, Some(message)), scope, None)),
        },
        Scope::Commit => Ok(stamp(ranged_file(&located.root, &Ends::LastCommit, &[], path, false).await, scope, None)),
        // Answered by `git_ops::uncommitted`, which the caller uses instead.
        Scope::Uncommitted => Ok(stamp(nothing(path, None), scope, None)),
    }
}

// ── which scopes are available ───────────────────────────────────────────

/// `session.git.scopes`: what the review screen may offer for this conversation right now.
pub(crate) async fn availability_rpc(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let session_id = p["sessionId"].as_str().filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("sessionId is required"))?;
    let detail = st.db.session_detail(session_id).map_err(|_| derr("not_found", "session not found", StatusCode::NOT_FOUND))?;
    Ok(availability(detail["projectPath"].as_str().filter(|path| !path.is_empty()), &detail).await)
}

fn entry(scope: &str, available: bool, reason: Option<&str>) -> Value {
    json!({"scope": scope, "available": available, "reason": reason})
}

/// Every scope, whether it has something to show, and a plain reason for each that does not, so
/// the window can hide what would be empty.
pub(crate) async fn availability(project_path: Option<&str>, detail: &Value) -> Value {
    let session_id = detail["id"].as_str().unwrap_or("");
    let recorded: Vec<&Value> = detail["files"].as_array().map(|files| files.iter().filter(|file| file["turnId"].as_str().is_some_and(|id| !id.is_empty())).collect()).unwrap_or_default();
    let last_turn = recorded.last().and_then(|file| file["turnId"].as_str()).map(str::to_owned);
    let turn = if recorded.is_empty() {
        json!({"scope": "turn", "available": false, "reason": "This conversation has no recorded turn changes.", "lastTurnId": Value::Null})
    } else {
        json!({"scope": "turn", "available": true, "reason": Value::Null, "lastTurnId": last_turn})
    };
    let Some(project) = project_path else {
        return json!({"sessionId": session_id, "scopes": [
            entry("uncommitted", false, Some("This conversation has no project folder.")),
            turn,
            entry("branch", false, Some("This conversation has no project folder.")),
            entry("commit", false, Some("This conversation has no project folder.")),
        ]});
    };
    let located = match locate(project).await {
        Ok(located) => located,
        Err(message) => {
            return json!({"sessionId": session_id, "scopes": [
                entry("uncommitted", false, Some(message)),
                turn,
                entry("branch", false, Some(message)),
                entry("commit", false, Some(message)),
            ]});
        }
    };
    let branch = match base_of(detail, &located.root).await {
        Ok(base) => json!({"scope": "branch", "available": true, "reason": Value::Null, "base": base.json()}),
        Err(message) => json!({"scope": "branch", "available": false, "reason": message, "base": Value::Null}),
    };
    let commit = if located.unborn {
        entry("commit", false, Some("This repository has no commits yet."))
    } else if commit_parents(&located.root).await > 1 {
        entry("commit", false, Some("The last commit merges other branches, so there is no single diff to show."))
    } else {
        entry("commit", true, None)
    };
    json!({"sessionId": session_id, "scopes": [entry("uncommitted", true, None), turn, branch, commit]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = crate::experiments::git_command().arg("-C").arg(dir).args(args).output().unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("retinue-scopes-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        git(&dir, &["config", "core.autocrlf", "false"]);
        dir
    }

    fn write(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).unwrap(); }
        std::fs::write(path, content).unwrap();
    }

    fn commit_all(dir: &Path, message: &str) {
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    /// An idle conversation in `dir`, with no recorded changes yet.
    fn setup(dir: &Path) -> AppState {
        let (st, _) = crate::phase2a_tests::state();
        st.db.create_session("review", "rt-test", "codex", &dir.to_string_lossy(), "Review").unwrap();
        st.db.update_session_state("review", "completed").unwrap();
        st
    }

    async fn call(st: &AppState, method: &str, p: Value) -> Result<Value, DispatchError> {
        crate::dispatch(st, method, p).await
    }

    /// One provider file change, as the conversation records it: path, kind and the saved diff.
    fn change_raw(path: &str, kind: &str, diff: &str) -> Value {
        json!({"changes": [{"path": path, "kind": kind, "diff": diff}]})
    }

    fn modified_patch(path: &str, before: &str, after: &str) -> String {
        format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-{before}\n+{after}\n")
    }

    fn file_of<'a>(view: &'a Value, path: &str) -> Option<&'a Value> {
        view["files"].as_array().unwrap().iter().find(|file| file["repositoryPath"] == path)
    }

    #[tokio::test]
    async fn uncommitted_is_answered_exactly_as_before() {
        let dir = repo("uncommitted");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "init");
        write(&dir, "a.txt", "two\n");
        write(&dir, "fresh.txt", "fresh\n");
        let st = setup(&dir);
        let plain = call(&st, "session.repository", json!({"sessionId": "review"})).await.unwrap();
        let asked = call(&st, "session.repository", json!({"sessionId": "review", "scope": "uncommitted"})).await.unwrap();
        assert_eq!(plain["files"], asked["files"], "an absent scope is uncommitted");
        assert_eq!(asked["scope"], "uncommitted");
        assert!(asked["base"].is_null());
        assert_eq!(plain["gitRepo"], true);
        assert_eq!(plain["branch"], "main");
        assert_eq!(file_of(&plain, "a.txt").unwrap()["addedLines"], 1);
        assert_eq!(file_of(&plain, "fresh.txt").unwrap()["change"], "added");
        let plain_diff = call(&st, "session.git.diff", json!({"sessionId": "review", "repositoryPath": "a.txt"})).await.unwrap();
        let asked_diff = call(&st, "session.git.diff", json!({"sessionId": "review", "repositoryPath": "a.txt", "scope": "uncommitted"})).await.unwrap();
        assert_eq!(plain_diff["diff"], asked_diff["diff"]);
        assert_eq!(asked_diff["change"], "modified");
        assert_eq!(asked_diff["addedLines"], 1);
        assert_eq!(asked_diff["removedLines"], 1);
        assert_eq!(asked_diff["scope"], "uncommitted");
        assert!(asked_diff["diff"].as_str().unwrap().contains("+two"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn every_scope_is_named_and_refuses_a_bad_one() {
        let dir = repo("parse");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "init");
        let st = setup(&dir);
        for (p, code) in [
            (json!({"sessionId": "review", "scope": "sideways"}), "invalid_argument"),
            (json!({"sessionId": "review", "scope": "turn"}), "invalid_argument"),
            (json!({"sessionId": "review", "scope": "turn", "turnId": "  "}), "invalid_argument"),
        ] {
            assert_eq!(call(&st, "session.repository", p.clone()).await.unwrap_err().1.code, code, "{p}");
            assert_eq!(call(&st, "session.git.diff", p).await.unwrap_err().1.code, code);
        }
        assert_eq!(call(&st, "session.repository", json!({"sessionId": "review", "scope": "branch"})).await.unwrap()["scope"], "branch");
        assert_eq!(call(&st, "session.repository", json!({"sessionId": "review", "scope": "commit"})).await.unwrap()["scope"], "commit");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_turn_lists_the_files_it_changed_and_their_diffs() {
        let dir = repo("turn");
        write(&dir, "a.txt", "one\n");
        write(&dir, "keep.txt", "keep\n");
        commit_all(&dir, "init");
        write(&dir, "a.txt", "two\n");
        let st = setup(&dir);
        st.db.insert_file_in_turn("review", Some("t1"), "a.txt", &change_raw("a.txt", "modify", &modified_patch("a.txt", "one", "two"))).unwrap();
        let view = call(&st, "session.repository", json!({"sessionId": "review", "scope": "turn", "turnId": "t1"})).await.unwrap();
        assert_eq!(view["scope"], "turn");
        assert_eq!(view["gitRepo"], true);
        let file = file_of(&view, "a.txt").unwrap();
        assert_eq!(file["change"], "modified");
        assert_eq!(file["state"], "available");
        assert!(file["reason"].is_null());
        assert_eq!(file["addedLines"], 1);
        assert_eq!(file["removedLines"], 1);
        assert!(file["diff"].as_str().unwrap().contains("+two"));
        assert!(view["path"] != Value::Null || true);
        assert_eq!(view["addedLines"], 1);
        assert_eq!(view["removedLines"], 1);
        assert_eq!(view["files"].as_array().unwrap().len(), 1, "a file the turn did not change is not listed");
        let whole = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "turn", "turnId": "t1", "repositoryPath": "a.txt"})).await.unwrap();
        assert_eq!(whole["scope"], "turn");
        assert_eq!(whole["change"], "modified");
        assert_eq!(whole["state"], "available");
        assert!(whole["diff"].as_str().unwrap().contains("+two"));
        // A file outside the turn, and a turn with nothing recorded.
        let other = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "turn", "turnId": "t1", "repositoryPath": "keep.txt"})).await.unwrap();
        assert!(other["change"].is_null() && other["diff"].is_null());
        assert_eq!(other["reason"], "This file is not part of that turn.");
        let absent = call(&st, "session.repository", json!({"sessionId": "review", "scope": "turn", "turnId": "gone"})).await.unwrap();
        assert!(absent["files"].as_array().unwrap().is_empty());
        assert_eq!(absent["error"], NO_TURN);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_turn_in_a_nested_project_folder_reports_the_repository_path() {
        let dir = repo("nested");
        write(&dir, "sub/a.txt", "one\n");
        commit_all(&dir, "init");
        write(&dir, "sub/a.txt", "two\n");
        let (st, _) = crate::phase2a_tests::state();
        let project = dir.join("sub");
        st.db.create_session("review", "rt-test", "codex", &project.to_string_lossy(), "Review").unwrap();
        st.db.update_session_state("review", "completed").unwrap();
        st.db.insert_file_in_turn("review", Some("t1"), "a.txt", &change_raw("a.txt", "modify", &modified_patch("a.txt", "one", "two"))).unwrap();
        let view = call(&st, "session.repository", json!({"sessionId": "review", "scope": "turn", "turnId": "t1"})).await.unwrap();
        let file = file_of(&view, "sub/a.txt").expect("a project-relative path becomes a repository path");
        assert_eq!(file["state"], "available");
        assert_eq!(file["path"], display_path(&project.join("a.txt")));
        let whole = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "turn", "turnId": "t1", "repositoryPath": "sub/a.txt"})).await.unwrap();
        assert!(whole["diff"].as_str().unwrap().contains("+two"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_turn_whose_file_changed_afterwards_says_why_it_cannot_show_it() {
        let dir = repo("turn-later");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "init");
        write(&dir, "a.txt", "two\n");
        let st = setup(&dir);
        st.db.insert_file_in_turn("review", Some("t1"), "a.txt", &change_raw("a.txt", "modify", &modified_patch("a.txt", "one", "two"))).unwrap();
        write(&dir, "a.txt", "three\n");
        let view = call(&st, "session.repository", json!({"sessionId": "review", "scope": "turn", "turnId": "t1"})).await.unwrap();
        let file = file_of(&view, "a.txt").unwrap();
        assert_eq!(file["state"], "blocked");
        assert!(file["diff"].is_null(), "a wrong diff is never shown");
        assert_eq!(file["reason"], "This file has changed since that turn, so its change from that turn can't be shown.");
        let whole = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "turn", "turnId": "t1", "repositoryPath": "a.txt"})).await.unwrap();
        assert!(whole["diff"].is_null());
        assert_eq!(whole["state"], "blocked");
        // A change whose saved diff was clipped can never be shown either.
        let st = setup(&dir);
        st.db.insert_file_in_turn("review", Some("t2"), "a.txt", &json!({"changes": [{"path": "a.txt", "kind": "modify", "diff": modified_patch("a.txt", "one", "three"), "clipped": true}]})).unwrap();
        let clipped = call(&st, "session.repository", json!({"sessionId": "review", "scope": "turn", "turnId": "t2"})).await.unwrap();
        let file = file_of(&clipped, "a.txt").unwrap();
        assert_eq!(file["state"], "blocked");
        assert_eq!(file["diffComplete"], false);
        assert_eq!(file["reason"], "No complete diff was saved for this change, so it can't be shown.");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn branch_compares_the_merge_base_and_includes_uncommitted_work() {
        let dir = repo("branch");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "base");
        let base_commit = git(&dir, &["rev-parse", "HEAD"]);
        git(&dir, &["checkout", "-q", "-b", "feature"]);
        write(&dir, "a.txt", "two\n");
        write(&dir, "b.txt", "b\n");
        commit_all(&dir, "feature work");
        write(&dir, "a.txt", "three\n");
        write(&dir, "u.txt", "u\n");
        let st = setup(&dir);
        let view = call(&st, "session.repository", json!({"sessionId": "review", "scope": "branch"})).await.unwrap();
        assert_eq!(view["scope"], "branch");
        assert_eq!(view["branch"], "feature");
        assert_eq!(view["base"]["ref"], "main");
        assert_eq!(view["base"]["kind"], "localDefault");
        assert_eq!(view["base"]["commit"], base_commit, "the point the branch left main");
        assert!(view["base"]["how"].as_str().unwrap().contains("main"));
        let a = file_of(&view, "a.txt").unwrap();
        assert_eq!(a["change"], "modified");
        assert_eq!((a["addedLines"].clone(), a["removedLines"].clone()), (json!(1), json!(1)), "the whole way from the base to the working tree");
        assert!(a["diff"].as_str().unwrap().contains("+three") && a["diff"].as_str().unwrap().contains("-one"));
        assert_eq!(file_of(&view, "b.txt").unwrap()["addedLines"], 1, "a commit on the branch");
        assert_eq!(file_of(&view, "u.txt").unwrap()["change"], "added", "uncommitted work is included");
        assert_eq!(view["addedLines"], 3);
        assert_eq!(view["removedLines"], 1);
        let committed = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "branch", "repositoryPath": "b.txt"})).await.unwrap();
        assert!(committed["diff"].as_str().unwrap().contains("+b"));
        assert_eq!(committed["base"]["ref"], "main");
        let untracked = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "branch", "repositoryPath": "u.txt"})).await.unwrap();
        assert_eq!(untracked["change"], "added");
        assert!(untracked["diff"].as_str().unwrap().contains("+u"));
        // A conversation in its own task copy uses the branch that copy was created from.
        let copy = std::env::temp_dir().join(format!("retinue-scopes-copy-{}", uuid::Uuid::new_v4()));
        git(&dir, &["worktree", "add", "-q", "-b", "retinue/turn-copy", copy.to_str().unwrap(), "HEAD"]);
        let st = setup(&copy);
        st.db.set_session_link("review", &json!({"kind": "side", "worktree": {"repo": dir.to_string_lossy(), "path": copy.to_string_lossy(), "branch": "retinue/turn-copy", "base": "feature", "baseCommit": base_commit}})).unwrap();
        let own = call(&st, "session.repository", json!({"sessionId": "review", "scope": "branch"})).await.unwrap();
        assert_eq!(own["base"]["ref"], "feature");
        assert_eq!(own["base"]["kind"], "worktreeBase");
        assert_eq!(own["base"]["commit"], git(&dir, &["rev-parse", "feature"]), "the copy's own base is the merge base");
        assert!(own["files"].as_array().unwrap().is_empty(), "a fresh copy has no changes");
        git(&dir, &["worktree", "remove", "--force", copy.to_str().unwrap()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn commit_compares_the_last_commit_with_its_parent() {
        let dir = repo("commit");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "first");
        write(&dir, "a.txt", "two\n");
        write(&dir, "b.txt", "b\n");
        commit_all(&dir, "second");
        write(&dir, "a.txt", "three\n");
        write(&dir, "later.txt", "later\n");
        let st = setup(&dir);
        let view = call(&st, "session.repository", json!({"sessionId": "review", "scope": "commit"})).await.unwrap();
        assert_eq!(view["scope"], "commit");
        assert!(view["base"].is_null());
        let a = file_of(&view, "a.txt").unwrap();
        assert_eq!(a["change"], "modified");
        assert!(a["diff"].as_str().unwrap().contains("+two"));
        assert!(!a["diff"].as_str().unwrap().contains("three"), "uncommitted work is not part of the last commit");
        assert_eq!(file_of(&view, "b.txt").unwrap()["change"], "added");
        assert!(file_of(&view, "later.txt").is_none(), "an untracked file is not committed");
        let whole = call(&st, "session.git.diff", json!({"sessionId": "review", "scope": "commit", "repositoryPath": "a.txt"})).await.unwrap();
        assert_eq!(whole["addedLines"], 1);
        assert_eq!(whole["removedLines"], 1);
        assert!(whole["diff"].as_str().unwrap().contains("+two"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_root_commit_and_a_merge_commit_answer_plainly() {
        let dir = repo("commit-root");
        write(&dir, "first.txt", "one\n");
        commit_all(&dir, "only commit");
        let st = setup(&dir);
        let root = call(&st, "session.repository", json!({"sessionId": "review", "scope": "commit"})).await.unwrap();
        assert_eq!(file_of(&root, "first.txt").unwrap()["change"], "added", "a root commit is compared against nothing");
        assert!(root["error"].is_null());
        // A merge has no single diff, and says so rather than showing one side.
        git(&dir, &["checkout", "-q", "-b", "side"]);
        write(&dir, "side.txt", "side\n");
        commit_all(&dir, "side work");
        git(&dir, &["checkout", "-q", "main"]);
        write(&dir, "main.txt", "main\n");
        commit_all(&dir, "main work");
        git(&dir, &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
        let merge = call(&st, "session.repository", json!({"sessionId": "review", "scope": "commit"})).await.unwrap();
        assert!(merge["files"].as_array().unwrap().is_empty());
        assert_eq!(merge["error"], "The last commit merges other branches, so there is no single diff to show.");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_folder_that_is_not_a_repository_answers_as_it_did() {
        let plain = std::env::temp_dir().join(format!("retinue-scopes-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&plain).unwrap();
        let st = setup(&plain);
        for (scope, turn_id) in [("branch", Value::Null), ("commit", Value::Null), ("turn", json!("t1"))] {
            let mut body = json!({"sessionId": "review", "scope": scope});
            if !turn_id.is_null() { body["turnId"] = turn_id; }
            let view = call(&st, "session.repository", body.clone()).await.unwrap();
            assert_eq!(view["gitRepo"], false, "{body}");
            assert_eq!(view["error"], "This project is not a git repository.");
            let mut diff_body = body.clone();
            diff_body["repositoryPath"] = json!("a.txt");
            assert_eq!(call(&st, "session.git.diff", diff_body).await.unwrap_err().1.message, "This project is not a git repository.");
        }
        let _ = std::fs::remove_dir_all(plain);
    }

    #[tokio::test]
    async fn the_availability_read_says_which_scopes_have_something_to_show() {
        let dir = repo("available");
        write(&dir, "a.txt", "one\n");
        commit_all(&dir, "init");
        write(&dir, "a.txt", "two\n");
        let st = setup(&dir);
        let before = call(&st, "session.git.scopes", json!({"sessionId": "review"})).await.unwrap();
        assert_eq!(before["sessionId"], "review");
        let scopes = before["scopes"].as_array().unwrap();
        let of = |name: &str| scopes.iter().find(|entry| entry["scope"] == name).unwrap().clone();
        assert_eq!(of("uncommitted")["available"], true);
        assert!(of("uncommitted")["reason"].is_null());
        assert_eq!(of("turn")["available"], false);
        assert_eq!(of("turn")["reason"], "This conversation has no recorded turn changes.");
        assert!(of("turn")["lastTurnId"].is_null());
        assert_eq!(of("branch")["available"], true);
        assert_eq!(of("branch")["base"]["ref"], "main");
        assert_eq!(of("commit")["available"], true);
        st.db.insert_file_in_turn("review", Some("t1"), "a.txt", &change_raw("a.txt", "modify", &modified_patch("a.txt", "one", "two"))).unwrap();
        let after = call(&st, "session.git.scopes", json!({"sessionId": "review"})).await.unwrap();
        let turn = after["scopes"].as_array().unwrap().iter().find(|entry| entry["scope"] == "turn").unwrap().clone();
        assert_eq!(turn["available"], true);
        assert_eq!(turn["lastTurnId"], "t1");
        // A folder that is not a repository has nothing to compare.
        let plain = std::env::temp_dir().join(format!("retinue-scopes-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&plain).unwrap();
        let st = setup(&plain);
        st.db.insert_file_in_turn("review", Some("t1"), "a.txt", &change_raw("a.txt", "modify", &modified_patch("a.txt", "one", "two"))).unwrap();
        let none = call(&st, "session.git.scopes", json!({"sessionId": "review"})).await.unwrap();
        let scopes = none["scopes"].as_array().unwrap();
        for name in ["uncommitted", "branch", "commit"] {
            let entry = scopes.iter().find(|entry| entry["scope"] == name).unwrap();
            assert_eq!(entry["available"], false, "{name}");
            assert_eq!(entry["reason"], "This project is not a git repository.");
        }
        assert_eq!(scopes.iter().find(|entry| entry["scope"] == "turn").unwrap()["available"], true, "recorded changes still exist without Git");
        assert_eq!(call(&st, "session.git.scopes", json!({"sessionId": "absent"})).await.unwrap_err().1.code, "not_found");
        assert_eq!(call(&st, "session.git.scopes", json!({})).await.unwrap_err().1.code, "invalid_argument");
        let _ = std::fs::remove_dir_all(plain);
        let _ = std::fs::remove_dir_all(dir);
    }
}

