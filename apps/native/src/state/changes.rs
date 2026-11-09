//! The changes model: diff parsing, the conversation's repository as Git
//! reports it, the per-turn checkpoints, and the pull request draft.
//!
//! Ported from `ui/diffParser.ts`, `ui/sessionRepository.ts`,
//! `ui/SessionRailExtras.tsx`, `ui/ChangeBanner.tsx` and
//! `ui/PullRequestDialog.tsx`. Nothing here writes to the repository: the daemon
//! takes the path from the conversation, and this window only names a
//! conversation and a path the repository itself listed.

use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Diffs
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    Meta,
    Hunk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
}

impl DiffLine {
    /// The line without its leading `+`, `-` or space.
    pub fn code(&self) -> &str {
        let mut characters = self.text.chars();
        characters.next();
        characters.as_str()
    }

    pub fn is_code(&self) -> bool {
        matches!(self.kind, LineKind::Context | LineKind::Added | LineKind::Removed)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedDiff {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub binary: bool,
    pub empty: bool,
}

/// One row of the side-by-side view: a removal and the addition that follows it
/// share a row, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SideRow {
    Hunk(String),
    Meta(String),
    Pair { left: Option<DiffLine>, right: Option<DiffLine> },
}

pub fn side_by_side_rows(lines: &[DiffLine]) -> Vec<SideRow> {
    let mut rows = Vec::new();
    let mut removed: Vec<DiffLine> = Vec::new();
    let mut added: Vec<DiffLine> = Vec::new();
    fn flush(removed: &mut Vec<DiffLine>, added: &mut Vec<DiffLine>, rows: &mut Vec<SideRow>) {
        let count = removed.len().max(added.len());
        for index in 0..count {
            rows.push(SideRow::Pair { left: removed.get(index).cloned(), right: added.get(index).cloned() });
        }
        removed.clear();
        added.clear();
    }
    for line in lines {
        match line.kind {
            LineKind::Removed => {
                // A removal after an addition starts a new change.
                if !added.is_empty() {
                    flush(&mut removed, &mut added, &mut rows);
                }
                removed.push(line.clone());
            }
            LineKind::Added => added.push(line.clone()),
            LineKind::Context => {
                flush(&mut removed, &mut added, &mut rows);
                rows.push(SideRow::Pair { left: Some(line.clone()), right: Some(line.clone()) });
            }
            LineKind::Hunk => {
                flush(&mut removed, &mut added, &mut rows);
                rows.push(SideRow::Hunk(line.text.clone()));
            }
            LineKind::Meta => {
                flush(&mut removed, &mut added, &mut rows);
                rows.push(SideRow::Meta(line.text.clone()));
            }
        }
    }
    flush(&mut removed, &mut added, &mut rows);
    rows
}

/// A hunk header as `git diff` writes it: `@@ -old,count +new,count @@`.
fn hunk_start(text: &str) -> Option<(u32, u32)> {
    let rest = text.strip_prefix("@@ -")?;
    let old_start: u32 = rest.split(|character: char| !character.is_ascii_digit()).next()?.parse().ok()?;
    let plus = rest.find(" +")?;
    let new_start: u32 = rest[plus + 2..].split(|character: char| !character.is_ascii_digit()).next()?.parse().ok()?;
    Some((old_start, new_start))
}

/// `parseUnifiedDiff`: every line of one file's diff, with the file's own line
/// numbers, and the counts a badge shows.
pub fn parse_unified_diff(content: &str) -> ParsedDiff {
    let mut lines = Vec::new();
    let mut old_number: Option<u32> = None;
    let mut new_number: Option<u32> = None;
    let mut additions = 0;
    let mut deletions = 0;
    let mut binary = false;
    for text in content.split('\n') {
        let text = text.strip_suffix('\r').unwrap_or(text);
        if binary {
            lines.push(DiffLine { kind: LineKind::Meta, text: text.to_owned(), old_number: None, new_number: None });
            continue;
        }
        if text.starts_with("Binary files ") && text.ends_with(" differ") || text.starts_with("GIT binary patch") {
            binary = true;
            lines.push(DiffLine { kind: LineKind::Meta, text: text.to_owned(), old_number: None, new_number: None });
            continue;
        }
        if text.starts_with("@@ ") {
            if let Some((old, new)) = hunk_start(text) {
                old_number = Some(old);
                new_number = Some(new);
                lines.push(DiffLine { kind: LineKind::Hunk, text: text.to_owned(), old_number: None, new_number: None });
                continue;
            }
        }
        if text.starts_with("\\ No newline at end of file") {
            lines.push(DiffLine { kind: LineKind::Meta, text: text.to_owned(), old_number: None, new_number: None });
            continue;
        }
        if text.starts_with("+++")
            || text.starts_with("---")
            || text.starts_with("diff --")
            || text.starts_with("index ")
            || text.starts_with("new file mode")
            || text.starts_with("deleted file mode")
        {
            lines.push(DiffLine { kind: LineKind::Meta, text: text.to_owned(), old_number: None, new_number: None });
            continue;
        }
        if text.starts_with('+') {
            lines.push(DiffLine { kind: LineKind::Added, text: text.to_owned(), old_number: None, new_number });
            additions += 1;
            if let Some(number) = new_number.as_mut() {
                *number += 1;
            }
            continue;
        }
        if text.starts_with('-') {
            lines.push(DiffLine { kind: LineKind::Removed, text: text.to_owned(), old_number, new_number: None });
            deletions += 1;
            if let Some(number) = old_number.as_mut() {
                *number += 1;
            }
            continue;
        }
        if text.starts_with(' ') {
            lines.push(DiffLine { kind: LineKind::Context, text: text.to_owned(), old_number, new_number });
            if let Some(number) = old_number.as_mut() {
                *number += 1;
            }
            if let Some(number) = new_number.as_mut() {
                *number += 1;
            }
            continue;
        }
        if !text.is_empty() {
            lines.push(DiffLine { kind: LineKind::Meta, text: text.to_owned(), old_number: None, new_number: None });
        }
    }
    ParsedDiff { lines, additions, deletions, binary, empty: content.is_empty() }
}

// ---------------------------------------------------------------------------
// The repository as Git reports it
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepositoryFile {
    /// Absolute path on this computer.
    pub path: String,
    /// Path relative to the repository root.
    pub repository_path: String,
    /// `added`, `modified`, `deleted` or `renamed`.
    pub change: String,
    pub added_lines: Option<i64>,
    pub removed_lines: Option<i64>,
    pub diff: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    /// `none`, `partial` or `all`.
    pub staged: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepositoryState {
    pub project_path: Option<String>,
    pub repo_root: Option<String>,
    pub git_repo: bool,
    pub branch: Option<String>,
    pub detached: bool,
    pub files: Vec<RepositoryFile>,
    pub added_lines: i64,
    pub removed_lines: i64,
    /// More files changed than the daemon lists.
    pub truncated: bool,
    pub error: Option<String>,
}

fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_owned)
}

fn count(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_f64).filter(|number| *number >= 0.0).map(|number| number as i64)
}

/// Accepts only a well-formed daemon reply; anything else is "unavailable",
/// never a guess (`normalizeRepository`).
pub fn normalize_repository(raw: &Value) -> Option<RepositoryState> {
    if !raw.is_object() {
        return None;
    }
    let mut files = Vec::new();
    if let Some(rows) = raw.get("files").and_then(Value::as_array) {
        for row in rows {
            if !row.is_object() {
                continue;
            }
            let repository_path = text(row.get("repositoryPath"));
            let path = text(row.get("path")).or_else(|| repository_path.clone());
            let Some(path) = path else { continue };
            let change = match text(row.get("change")).unwrap_or_else(|| "modified".to_owned()).as_str() {
                "added" | "modified" | "deleted" | "renamed" => text(row.get("change")).unwrap(),
                _ => "modified".to_owned(),
            };
            let staged = match text(row.get("staged")).unwrap_or_else(|| "none".to_owned()).as_str() {
                "all" | "partial" => text(row.get("staged")).unwrap(),
                _ => "none".to_owned(),
            };
            files.push(RepositoryFile {
                path,
                repository_path: repository_path.unwrap_or_default(),
                change,
                added_lines: count(row.get("addedLines")),
                removed_lines: count(row.get("removedLines")),
                diff: text(row.get("diff")),
                binary: row.get("binary").and_then(Value::as_bool).unwrap_or(false),
                truncated: row.get("truncated").and_then(Value::as_bool).unwrap_or(false),
                staged,
            });
        }
    }
    for file in files.iter_mut() {
        if file.repository_path.is_empty() {
            file.repository_path = file.path.clone();
        }
    }
    let added = count(raw.get("addedLines")).unwrap_or_else(|| files.iter().filter_map(|file| file.added_lines).sum());
    let removed = count(raw.get("removedLines")).unwrap_or_else(|| files.iter().filter_map(|file| file.removed_lines).sum());
    Some(RepositoryState {
        project_path: text(raw.get("projectPath")),
        repo_root: text(raw.get("repoRoot")),
        git_repo: raw.get("gitRepo").and_then(Value::as_bool).unwrap_or(false),
        branch: text(raw.get("branch")),
        detached: raw.get("detached").and_then(Value::as_bool).unwrap_or(false),
        files,
        added_lines: added,
        removed_lines: removed,
        truncated: raw.get("truncated").and_then(Value::as_bool).unwrap_or(false),
        error: text(raw.get("error")),
    })
}

/// A path in one comparable shape: forward slashes, no `.` or `..` parts.
pub fn normalize_path(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    let unc = slashed.starts_with("//");
    let absolute = !unc && slashed.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in slashed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if let Some(last) = parts.last() {
                if !(last.len() == 2 && last.ends_with(':')) {
                    parts.pop();
                }
            }
            continue;
        }
        parts.push(part);
    }
    let joined = parts.join("/");
    if unc {
        format!("//{joined}")
    } else if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// An absolute Windows or POSIX path.
pub fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/');
    drive || path.starts_with("\\\\") || path.starts_with('/')
}

fn windows_style(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':') || path.starts_with("\\\\") || path.contains('\\')
}

/// Whether two paths name the same file. A path the coding agent reported
/// relative to the project is resolved against `root` first.
pub fn same_path(left: &str, right: &str, root: Option<&str>) -> bool {
    let resolve = |path: &str| {
        let joined = match root {
            Some(root) if !is_absolute(path) => format!("{root}/{path}"),
            _ => path.to_owned(),
        };
        normalize_path(&joined)
    };
    let a = resolve(left);
    let b = resolve(right);
    if windows_style(&a) || windows_style(&b) || root.is_some_and(windows_style) {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

pub fn is_created(change: &str) -> bool {
    matches!(change.to_ascii_lowercase().as_str(), "added" | "create" | "created" | "new")
}

/// One row in the rail's Changes and Outputs cards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailFile {
    pub key: String,
    /// Absolute where known.
    pub path: String,
    pub name: String,
    /// Relative to the repository root, for display.
    pub relative: Option<String>,
    pub change: String,
    pub added: Option<i64>,
    pub removed: Option<i64>,
    pub diff: Option<String>,
    pub binary: bool,
    pub truncated: bool,
    /// `repository` (Git listed it) or `conversation` (a turn reported it).
    pub source: &'static str,
}

pub fn file_note(file: &RailFile) -> Option<&'static str> {
    if file.change == "deleted" {
        return Some("deleted");
    }
    if file.binary {
        return Some("binary");
    }
    if file.truncated {
        return Some("cut short");
    }
    if file.change == "renamed" {
        return Some("renamed");
    }
    None
}

/// Why a row cannot open a diff, in words; `None` when it can.
pub fn diff_unavailable_reason(file: &RailFile) -> Option<String> {
    if file.diff.is_some() {
        return None;
    }
    if file.binary {
        return Some("Binary file: there is no text diff to show.".to_owned());
    }
    if file.change == "deleted" {
        return Some("The file was deleted, so there is no text diff to show.".to_owned());
    }
    Some(if file.source == "repository" {
        "Git did not return a diff for this file. Open it to read the change.".to_owned()
    } else {
        "The coding agent did not report a diff for this file.".to_owned()
    })
}

/// What the rail's Changes card shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailView {
    /// Files Git reports as different from HEAD.
    pub repository: Vec<RailFile>,
    /// Files a turn reported that Git does not list now.
    pub earlier: Vec<RailFile>,
    /// New files, for the Outputs card.
    pub created: Vec<RailFile>,
    pub added: i64,
    pub removed: i64,
    /// Whether Git is the source of the list, so the card can say whose changes
    /// these are.
    pub from_repository: bool,
}

/// `buildRailView`: Git first, because it is the repository's own truth.
pub fn build_rail_view(repository: Option<&RepositoryState>) -> RailView {
    let usable = repository.is_some_and(|repository| repository.git_repo && repository.error.is_none());
    let Some(repository) = repository.filter(|_| usable) else { return RailView::default() };
    let rows: Vec<RailFile> = repository
        .files
        .iter()
        .map(|file| RailFile {
            key: format!("repo:{}", file.repository_path),
            path: file.path.clone(),
            name: file
                .repository_path
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .unwrap_or(&file.repository_path)
                .to_owned(),
            relative: Some(file.repository_path.clone()),
            change: file.change.clone(),
            added: file.added_lines,
            removed: file.removed_lines,
            diff: file.diff.clone(),
            binary: file.binary,
            truncated: file.truncated,
            source: "repository",
        })
        .collect();
    let created = rows.iter().filter(|file| is_created(&file.change)).cloned().collect();
    RailView {
        repository: rows,
        earlier: Vec::new(),
        created,
        added: repository.added_lines,
        removed: repository.removed_lines,
        from_repository: true,
    }
}

/// The message the Create PR dialog sends once the owner confirms: an ordinary
/// request to the conversation's coding agent.
pub fn pull_request_prompt(repository: &RepositoryState) -> String {
    let root = repository.repo_root.clone().or_else(|| repository.project_path.clone()).unwrap_or_else(|| "this repository".to_owned());
    let branch = if repository.detached {
        "a detached HEAD (no branch)".to_owned()
    } else {
        repository.branch.clone().map(|branch| format!("`{branch}`")).unwrap_or_else(|| "an unknown branch".to_owned())
    };
    let count = repository.files.len();
    let size = format!(
        "{count}{} changed {}, +{} −{}",
        if repository.truncated { "+" } else { "" },
        if count == 1 { "file" } else { "files" },
        repository.added_lines,
        repository.removed_lines
    );
    [
        "Please open a pull request for the uncommitted changes in this repository.".to_owned(),
        String::new(),
        format!("Repository: {root}"),
        format!("Current branch: {branch}"),
        format!("Changes: {size}"),
        String::new(),
        "1. Start by inspecting the working tree (git status and git diff) and read what changed before you do anything else.".to_owned(),
        "2. Leave unrelated work alone. Stage and commit only the changes that belong in this pull request, and tell me about any file you left out and why.".to_owned(),
        "3. If the current branch is main, master or the repository's default branch, create a suitably named branch first. Otherwise keep using the current branch.".to_owned(),
        "4. Commit with a clear message, push the branch and open the pull request with the GitHub CLI (gh) or the provider tooling installed on this computer. If a tool is missing or not signed in, stop and tell me what is needed instead of working around it.".to_owned(),
        "5. Reply with the pull request link, or with exactly what failed.".to_owned(),
    ]
    .join("\n")
}

// ---------------------------------------------------------------------------
// Workspace and checkpoints (`SessionRailExtras.tsx`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedWith {
    pub session_id: String,
    pub agent_id: Option<String>,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub base: String,
    pub base_commit: Option<String>,
    pub repo: Option<String>,
}

/// `session.workspace`: where the conversation works.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Workspace {
    pub session_id: String,
    /// `shared`, `isolated` or `unknown`.
    pub mode: String,
    pub project_path: Option<String>,
    pub is_git: bool,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub worktree: Option<Worktree>,
    pub isolation_failure: Option<String>,
    #[serde(default, deserialize_with = "crate::state::serde_defaults::null_as_default")]
    pub shared_with: Vec<SharedWith>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CheckpointCapability {
    pub file_undo: bool,
    pub file_undo_reason: Option<String>,
    pub provider_state_restore: bool,
    pub provider_reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CheckpointFile {
    pub id: String,
    pub turn_id: Option<String>,
    pub path: String,
    pub operation: Option<String>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    /// `available`, `reverted` or `blocked`.
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CheckpointTurn {
    pub turn_id: Option<String>,
    #[serde(default, deserialize_with = "crate::state::serde_defaults::null_as_default")]
    pub files: Vec<CheckpointFile>,
}

/// `session.checkpoints`: what each turn changed, and whether one change can
/// still be undone.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Checkpoints {
    pub session_id: String,
    pub capability: CheckpointCapability,
    pub busy: bool,
    #[serde(default, deserialize_with = "crate::state::serde_defaults::null_as_default")]
    pub turns: Vec<CheckpointTurn>,
}

impl Checkpoints {
    pub fn can_undo(&self) -> bool {
        self.capability.file_undo && !self.busy
    }

    /// The newest recorded turn, which is what the change banner shows.
    pub fn last_turn(&self) -> Option<&CheckpointTurn> {
        self.turns.last()
    }

    pub fn turn_added(&self, turn: &CheckpointTurn) -> i64 {
        turn.files.iter().filter_map(|file| file.additions).sum()
    }

    pub fn turn_removed(&self, turn: &CheckpointTurn) -> i64 {
        turn.files.iter().filter_map(|file| file.deletions).sum()
    }
}

/// A change the banner explains: what the newest turn touched, with the counts
/// the daemon recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BannerChange {
    pub id: String,
    pub path: String,
    pub name: String,
    pub added: i64,
    pub removed: i64,
    /// The daemon could not keep the whole diff, so the change cannot be undone
    /// from here.
    pub clipped: bool,
}

/// The banner's rows: the newest turn's files, once that turn has finished.
/// A conversation that is working shows nothing, as React does.
pub fn banner_changes(checkpoints: Option<&Checkpoints>, live: bool, dismissed: &[String]) -> Vec<BannerChange> {
    if live {
        return Vec::new();
    }
    let Some(turn) = checkpoints.and_then(Checkpoints::last_turn) else { return Vec::new() };
    let mut rows: Vec<BannerChange> = Vec::new();
    for file in &turn.files {
        let name = file.path.rsplit(['\\', '/']).next().unwrap_or(&file.path).to_owned();
        let change = BannerChange {
            id: file.id.clone(),
            path: file.path.clone(),
            name,
            added: file.additions.unwrap_or(0),
            removed: file.deletions.unwrap_or(0),
            clipped: file.reason.as_deref() == Some("diff_clipped"),
        };
        // One row per path, its latest change; a dismissal is about that row.
        match rows.iter_mut().find(|row| row.path == change.path) {
            Some(existing) => *existing = change,
            None => rows.push(change),
        }
    }
    rows.retain(|row| !dismissed.iter().any(|dismissed| dismissed == &row.id));
    rows
}

/// Above this share of the context window the banner suggests a fresh
/// conversation.
pub const CONTEXT_TIP_PERCENT: f64 = 80.0;

// ---------------------------------------------------------------------------
// The pull request draft (`session.pr.draft`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GitHubCli {
    pub available: bool,
    pub login: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PullRequestDraft {
    pub ready: bool,
    pub reason: Option<String>,
    pub gh: GitHubCli,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub commits_ahead: i64,
    pub uncommitted: i64,
    pub title: String,
    pub body: String,
    pub existing_url: Option<String>,
}

/// What the owner is approving, in order.
pub fn pull_request_plan(draft: &PullRequestDraft, commit_first: bool, as_draft: bool) -> Vec<String> {
    let plural = |count: i64, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let mut plan = Vec::new();
    if commit_first && draft.uncommitted > 0 {
        plan.push(format!("Commit {}", plural(draft.uncommitted, "uncommitted file")));
    }
    plan.push(format!("Push {} to origin", draft.branch.clone().unwrap_or_else(|| "this branch".to_owned())));
    plan.push(format!(
        "Open a {}pull request{}{}",
        if as_draft { "draft " } else { "" },
        draft.base.as_ref().map(|base| format!(" into {base}")).unwrap_or_default(),
        draft.gh.login.as_ref().map(|login| format!(" as {login}")).unwrap_or_default()
    ));
    plan
}

/// One step of a created pull request.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PullRequestStep {
    pub id: String,
    /// `done`, `failed` or `skipped`.
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PullRequestCreated {
    #[serde(default, deserialize_with = "crate::state::serde_defaults::null_as_default")]
    pub steps: Vec<PullRequestStep>,
    pub url: Option<String>,
}

pub fn pull_request_step_label(id: &str) -> String {
    match id {
        "commit" => "Commit the changes".to_owned(),
        "push" => "Push the branch".to_owned(),
        "pr" => "Open the pull request".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\nindex 111..222 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10,3 +10,4 @@ fn main() {\n context one\n-removed line\n+added line\n+another added\n context two\n";

    #[test]
    fn a_diff_is_parsed_with_both_files_line_numbers() {
        let parsed = parse_unified_diff(DIFF);
        assert_eq!((parsed.additions, parsed.deletions), (2, 1));
        assert!(!parsed.binary);
        assert!(!parsed.empty);
        let code: Vec<&DiffLine> = parsed.lines.iter().filter(|line| line.is_code()).collect();
        assert_eq!(code.len(), 5);
        // The hunk starts at line 10 in both files.
        assert_eq!((code[0].old_number, code[0].new_number), (Some(10), Some(10)));
        assert_eq!(code[0].code(), "context one");
        assert_eq!((code[1].old_number, code[1].new_number), (Some(11), None));
        assert!(code[1].kind == LineKind::Removed);
        assert_eq!((code[2].old_number, code[2].new_number), (None, Some(11)));
        assert_eq!((code[3].old_number, code[3].new_number), (None, Some(12)));
        assert_eq!((code[4].old_number, code[4].new_number), (Some(12), Some(13)));
    }

    #[test]
    fn a_removal_counts_in_the_old_file_and_an_addition_in_the_new_one() {
        let parsed = parse_unified_diff(DIFF);
        assert_eq!(parsed.lines.iter().filter(|line| line.kind == LineKind::Removed).count(), 1);
        assert_eq!(
            parsed.lines.iter().filter(|line| line.new_number == Some(11) || line.old_number == Some(11)).count(),
            2,
            "a removed line 11 and an added line 11"
        );
    }

    #[test]
    fn a_binary_diff_says_so_and_keeps_its_lines_as_notes() {
        let parsed = parse_unified_diff("diff --git a/logo.png b/logo.png\nBinary files a/logo.png and b/logo.png differ\n");
        assert!(parsed.binary);
        assert_eq!(parsed.additions, 0);
        assert!(parsed.lines.iter().any(|line| line.text.starts_with("Binary files")));
    }

    #[test]
    fn an_empty_diff_is_empty_and_a_blank_line_is_not_a_line() {
        assert!(parse_unified_diff("").empty);
        let parsed = parse_unified_diff("@@ -1,1 +1,1 @@\n\n context\n");
        assert_eq!(parsed.lines.iter().filter(|line| line.is_code()).count(), 1, "the blank line between hunks is not a line");
    }

    #[test]
    fn a_new_file_has_no_old_line_numbers() {
        let parsed = parse_unified_diff("@@ -0,0 +1,2 @@\n+one\n+two\n");
        assert_eq!(parsed.additions, 2);
        let added: Vec<&DiffLine> = parsed.lines.iter().filter(|line| line.kind == LineKind::Added).collect();
        assert_eq!(added[0].new_number, Some(1));
        assert_eq!(added[1].new_number, Some(2));
        assert_eq!(added[0].old_number, None);
    }

    #[test]
    fn the_side_by_side_view_pairs_a_removal_with_the_addition_under_it() {
        let parsed = parse_unified_diff(DIFF);
        let rows = side_by_side_rows(&parsed.lines);
        let pairs: Vec<&SideRow> = rows.iter().filter(|row| matches!(row, SideRow::Pair { .. })).collect();
        // context one, (removed + two additions => 2 rows), context two
        assert_eq!(pairs.len(), 4);
        match pairs[1] {
            SideRow::Pair { left, right } => {
                assert_eq!(left.as_ref().unwrap().kind, LineKind::Removed);
                assert_eq!(right.as_ref().unwrap().code(), "added line");
            }
            other => panic!("expected a pair, got {other:?}"),
        }
        match pairs[2] {
            SideRow::Pair { left, right } => {
                assert!(left.is_none(), "the second addition has nothing to pair with");
                assert_eq!(right.as_ref().unwrap().code(), "another added");
            }
            other => panic!("expected a pair, got {other:?}"),
        }
    }

        fn repository() -> Value {
        json!({
            "projectPath": "D:/work/app",
            "repoRoot": "D:/work/app",
            "gitRepo": true,
            "branch": "main",
            "detached": false,
            "addedLines": 3,
            "removedLines": 1,
            "files": [
                { "path": "D:/work/app/src/a.rs", "repositoryPath": "src/a.rs", "change": "modified", "addedLines": 2, "removedLines": 1, "staged": "none", "diff": DIFF },
                { "path": "D:/work/app/src/new.rs", "repositoryPath": "src/new.rs", "change": "added", "addedLines": 1, "removedLines": 0, "staged": "all" },
                { "path": "D:/work/app/logo.png", "repositoryPath": "logo.png", "change": "modified", "binary": true }
            ]
        })
    }

    #[test]
    fn a_repository_reply_is_read_without_gaps() {
        let state = normalize_repository(&repository()).expect("a repository");
        assert!(state.git_repo);
        assert_eq!(state.branch.as_deref(), Some("main"));
        assert_eq!(state.files.len(), 3);
        assert_eq!(state.files[0].repository_path, "src/a.rs");
        assert_eq!(state.files[1].staged, "all");
        assert!(state.files[2].binary);
        assert_eq!((state.added_lines, state.removed_lines), (3, 1));
        assert!(normalize_repository(&json!("not an object")).is_none());
    }

    #[test]
    fn the_rail_shows_git_first_and_calls_out_new_files() {
        let state = normalize_repository(&repository()).unwrap();
        let view = build_rail_view(Some(&state));
        assert!(view.from_repository);
        assert_eq!(view.repository.len(), 3);
        assert_eq!(view.created.len(), 1);
        assert_eq!(view.created[0].relative.as_deref(), Some("src/new.rs"));
        assert_eq!((view.added, view.removed), (3, 1));
        assert_eq!(view.repository[0].name, "a.rs");
        assert_eq!(file_note(&view.repository[2]), Some("binary"));
    }

    #[test]
    fn a_folder_that_is_not_a_repository_has_no_rows() {
        let state = RepositoryState { git_repo: false, ..RepositoryState::default() };
        let view = build_rail_view(Some(&state));
        assert!(!view.from_repository);
        assert!(view.repository.is_empty());
        let error = RepositoryState { git_repo: true, error: Some("git failed".into()), ..RepositoryState::default() };
        assert!(!build_rail_view(Some(&error)).from_repository);
    }

    #[test]
    fn a_chip_that_cannot_open_a_diff_says_why() {
        let file = RailFile {
            key: "k".into(),
            path: "D:/a/b.png".into(),
            name: "b.png".into(),
            relative: Some("b.png".into()),
            change: "modified".into(),
            added: None,
            removed: None,
            diff: None,
            binary: true,
            truncated: false,
            source: "repository",
        };
        assert!(diff_unavailable_reason(&file).unwrap().contains("Binary"));
        let with_diff = RailFile { diff: Some("@@ -1 +1 @@".into()), ..file };
        assert!(diff_unavailable_reason(&with_diff).is_none());
    }

    #[test]
    fn two_paths_name_one_file_when_one_is_relative_to_the_project() {
        assert!(same_path("src/a.rs", "D:/work/app/src/a.rs", Some("D:/work/app")));
        assert!(same_path("D:\\Work\\App\\src\\a.rs", "d:/work/app/src/a.rs", None), "Windows paths ignore case");
        assert!(!same_path("src/a.rs", "src/b.rs", Some("D:/work/app")));
        assert_eq!(normalize_path("D:/work/app/./src/../src/a.rs"), "D:/work/app/src/a.rs");
        assert!(is_absolute("D:/x") && is_absolute("/x") && !is_absolute("src/x"));
    }

    #[test]
    fn checkpoints_give_the_banner_the_newest_turn() {
        let checkpoints: Checkpoints = serde_json::from_value(json!({
            "sessionId": "s1",
            "capability": { "fileUndo": true, "fileUndoReason": null, "providerStateRestore": false, "providerReason": "The provider cannot restore its own state." },
            "busy": false,
            "turns": [
                { "turnId": "t1", "files": [ { "id": "f1", "path": "src/a.rs", "state": "available", "additions": 2, "deletions": 1 } ] },
                { "turnId": "t2", "files": [
                    { "id": "f2", "path": "src/b.rs", "state": "available", "additions": 4, "deletions": 0 },
                    { "id": "f3", "path": "src/b.rs", "state": "blocked", "additions": 1, "deletions": 1, "reason": "diff_clipped" }
                ] }
            ]
        })).unwrap();
        assert!(checkpoints.can_undo());
        let turn = checkpoints.last_turn().unwrap();
        assert_eq!(checkpoints.turn_added(turn), 5);
        assert_eq!(checkpoints.turn_removed(turn), 1);
        let rows = banner_changes(Some(&checkpoints), false, &[]);
        assert_eq!(rows.len(), 1, "one row per path, its latest change");
        assert_eq!(rows[0].id, "f3");
        assert!(rows[0].clipped);
        assert!(banner_changes(Some(&checkpoints), true, &[]).is_empty(), "a working conversation shows no banner");
        assert!(banner_changes(Some(&checkpoints), false, &["f3".to_owned()]).is_empty(), "a dismissed row stays hidden");
    }

    #[test]
    fn a_pull_request_plan_reads_in_order() {
        let draft = PullRequestDraft {
            ready: true,
            branch: Some("fix/dock".into()),
            base: Some("main".into()),
            uncommitted: 2,
            gh: GitHubCli { available: true, login: Some("basem".into()) },
            ..PullRequestDraft::default()
        };
        assert_eq!(
            pull_request_plan(&draft, true, false),
            vec![
                "Commit 2 uncommitted files".to_owned(),
                "Push fix/dock to origin".to_owned(),
                "Open a pull request into main as basem".to_owned()
            ]
        );
        let one = PullRequestDraft { uncommitted: 1, ..draft.clone() };
        assert_eq!(pull_request_plan(&one, true, true)[0], "Commit 1 uncommitted file");
        assert!(pull_request_plan(&draft, false, false)[0].starts_with("Push"));
    }

    #[test]
    fn the_ask_the_bot_message_names_the_branch_and_the_changes() {
        let state = normalize_repository(&repository()).unwrap();
        let prompt = pull_request_prompt(&state);
        assert!(prompt.contains("Repository: D:/work/app"));
        assert!(prompt.contains("Current branch: `main`"));
        assert!(prompt.contains("3 changed files, +3 −1"));
        assert!(prompt.contains("main, master"), "the bot is told to branch off the default branch");
    }

    #[test]
    fn a_detached_head_is_said_in_words() {
        let state = RepositoryState { git_repo: true, detached: true, ..RepositoryState::default() };
        assert!(pull_request_prompt(&state).contains("a detached HEAD (no branch)"));
    }
}
