//! FIX39 P10: git worktrees for parallel code tasks.
//!
//! When a code-writing delegation targets a git project where another task is
//! already working, the daemon runs it in its own linked worktree (a sibling
//! folder, on a `retinue/<slug>` branch) so the two tasks cannot clash. Nothing
//! here touches the owner's uncommitted changes: a worktree is created from
//! `HEAD`, and a merge back is refused while the main checkout is dirty. Every
//! git call is a plain argument list — never a shell — and its output is read
//! only to branch on it, never logged.

use std::path::{Path, PathBuf};

/// The longest a worktree branch slug may be; the folder name matches.
const MAX_SLUG: usize = 40;
/// The folder family holds every task copy of one repository: `<repo>-retinue`.
const FAMILY_SUFFIX: &str = "-retinue";
/// The names a task copy tries before one is free: the topic itself, then
/// `-2` … `-20`, then a short random suffix. The family can already hold every
/// numbered name when many conversations start with the same few words.
const NUMBERED_NAMES: usize = 20;
const RANDOM_NAMES: usize = 8;
/// How many characters a random name suffix takes.
const SUFFIX_CHARS: usize = 4;
/// Every name a task copy may try, numbered then random.
pub(crate) const NAME_ATTEMPTS: usize = NUMBERED_NAMES + RANDOM_NAMES;

/// A created task copy: the repository it belongs to, where it lives, the branch
/// checked out there, the branch it was taken from, and the commit it started at.
#[derive(Clone, Debug)]
pub(crate) struct Worktree {
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
    pub base_commit: String,
}

impl Worktree {
    /// The record stored on the side conversation's link. It carries no
    /// conversation text — only paths, a branch and a commit.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "repo": native_string(&self.repo),
            "path": native_string(&self.path),
            "branch": self.branch,
            "base": self.base,
            "baseCommit": self.base_commit,
        })
    }
}

/// A path in one native form: backslashes on Windows, unchanged elsewhere.
///
/// Git prints `C:/…` and a joined child uses the platform separator, so a record
/// returned to a model would otherwise read `C:/a\b` (owner report, 21 November
/// 2026). This is the one place that form is made.
pub(crate) fn native_string(path: &Path) -> String {
    #[cfg(windows)]
    { path.to_string_lossy().replace('/', "\\") }
    #[cfg(not(windows))]
    { path.to_string_lossy().into_owned() }
}

/// What a task copy changed against its base commit, for the merge card.
#[derive(Clone, Debug, Default)]
pub(crate) struct Summary {
    pub files: Vec<String>,
    pub insertions: u64,
    pub deletions: u64,
    pub untracked: u64,
}

impl Summary {
    /// A one-line diff summary, e.g. `3 files changed, +12 −4, 1 new`.
    pub(crate) fn line(&self) -> String {
        let count = self.files.len();
        let files = if count == 1 { "1 file changed".to_owned() } else { format!("{count} files changed") };
        let mut out = format!("{files}, +{} −{}", self.insertions, self.deletions);
        if self.untracked > 0 { out.push_str(&format!(", {} new", self.untracked)); }
        out
    }
}

/// One `git [-C dir] args...` run without a shell. Returns trimmed stdout on a
/// zero exit; an error string otherwise. Git's output is never logged.
fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = crate::experiments::git_command()
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|_| "Git could not be started.".to_owned())?;
    if !output.status.success() {
        return Err("Git did not complete that command.".to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The repository root for a folder inside it, as git reports it, or `None` when
/// the folder is missing or is not in a git repository. The path is left as git
/// prints it (absolute, forward slashes) so sibling worktree paths git accepts
/// are built from it; comparisons normalise case and separators separately.
pub(crate) fn repo_root(path: &Path) -> Option<PathBuf> {
    if !path.is_dir() { return None; }
    run(path, &["rev-parse", "--show-toplevel"]).ok().map(PathBuf::from)
}

/// A case- and separator-insensitive key for comparing two folders on Windows.
pub(crate) fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// The current branch of a repository, or an error on a detached HEAD (a task
/// copy must branch from a branch so the merge back is unambiguous).
fn current_branch(repo: &Path) -> Result<String, String> {
    let branch = run(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" { return Err("The repository is on a detached HEAD.".to_owned()); }
    Ok(branch)
}

/// Whether the working tree has any uncommitted or untracked change. Untracked
/// files count: a merge could overwrite one that the task also creates.
pub(crate) fn is_dirty(repo: &Path) -> Result<bool, String> {
    Ok(!run(repo, &["status", "--porcelain"])?.trim().is_empty())
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

/// A branch- and folder-safe slug for a task topic: lowercase ASCII letters and
/// digits with single dashes, at most 40 characters, never empty.
pub(crate) fn slug(topic: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for character in topic.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() { out.push(character); dash = false; }
        else if !dash && !out.is_empty() { out.push('-'); dash = true; }
        if out.len() >= MAX_SLUG { break; }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() { "task".to_owned() } else { trimmed.to_owned() }
}

/// The slug for one attempt: the topic itself, then `-2` … `-20`, then a short
/// random suffix rather than a refusal. The stem is cut so the suffix still fits
/// inside `MAX_SLUG`; `task` is the fallback when nothing is left.
pub(crate) fn candidate_slug(base: &str, attempt: usize) -> String {
    if attempt == 0 { return base.to_owned(); }
    if attempt < NUMBERED_NAMES { return format!("{base}-{}", attempt + 1); }
    let random = uuid::Uuid::new_v4().simple().to_string();
    let suffix = random.chars().take(SUFFIX_CHARS).collect::<String>();
    let keep = MAX_SLUG.saturating_sub(SUFFIX_CHARS + 1);
    let stem = base.chars().take(keep).collect::<String>();
    let stem = stem.trim_matches('-');
    format!("{}-{suffix}", if stem.is_empty() { "task" } else { stem })
}

/// Create `<repo>-retinue/<slug>` on a new `retinue/<slug>` branch from `HEAD`.
/// Returns `Err` (never panics) so the caller can fall back to the shared folder.
pub(crate) fn create(repo: &Path, topic: &str) -> Result<Worktree, String> {
    let repo = repo_root(repo).ok_or_else(|| "That folder is not a git repository.".to_owned())?;
    let base = current_branch(&repo)?;
    let base_commit = run(&repo, &["rev-parse", "HEAD"])?;
    let parent = repo.parent().ok_or_else(|| "The repository has no parent folder.".to_owned())?;
    let name = repo.file_name().and_then(|name| name.to_str()).unwrap_or("repo");
    let family = parent.join(format!("{name}{FAMILY_SUFFIX}"));
    std::fs::create_dir_all(&family).map_err(|error| format!("The task copy folder could not be created: {error}"))?;
    let base_slug = slug(topic);
    for attempt in 0..NAME_ATTEMPTS {
        let slug = candidate_slug(&base_slug, attempt);
        let branch = format!("retinue/{slug}");
        let path = family.join(&slug);
        if path.exists() || branch_exists(&repo, &branch) { continue; }
        let path_arg = path.to_string_lossy().into_owned();
        run(&repo, &["worktree", "add", "-b", &branch, &path_arg, &base_commit]).map_err(|_| "The task copy could not be created.".to_owned())?;
        return Ok(Worktree { repo, path, branch, base, base_commit });
    }
    Err("No free name was available for a task copy.".to_owned())
}

/// What the task copy changed against `base_commit`: tracked changes (committed
/// or not) plus untracked files, with an insertion/deletion count.
pub(crate) fn inspect(worktree: &Path, base_commit: &str) -> Result<Summary, String> {
    let base = if base_commit.trim().is_empty() { "HEAD" } else { base_commit };
    let mut files: Vec<String> = run(worktree, &["diff", "--name-only", base])?
        .lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect();
    let untracked: Vec<String> = run(worktree, &["ls-files", "--others", "--exclude-standard"])?
        .lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect();
    files.extend(untracked.iter().cloned());
    files.sort();
    files.dedup();
    let mut insertions = 0u64;
    let mut deletions = 0u64;
    for line in run(worktree, &["diff", "--numstat", base])?.lines() {
        let mut columns = line.split('\t');
        if let (Some(added), Some(removed)) = (columns.next(), columns.next()) {
            insertions += added.trim().parse().unwrap_or(0);
            deletions += removed.trim().parse().unwrap_or(0);
        }
    }
    Ok(Summary { files, insertions, deletions, untracked: untracked.len() as u64 })
}

/// How many commits the copy's branch has made since `base_commit` (wave 14,
/// `worktree_status`); `None` when the base is unknown or git cannot say.
pub(crate) fn commits_ahead(worktree: &Path, base_commit: &str) -> Option<u64> {
    if base_commit.trim().is_empty() { return None; }
    run(worktree, &["rev-list", "--count", &format!("{}..HEAD", base_commit.trim())]).ok()?.parse().ok()
}

/// Commit everything in the task copy under a fixed Retinue identity, so a merge
/// back has something to carry. A no-op when the copy is already clean.
pub(crate) fn commit_all(worktree: &Path, message: &str) -> Result<(), String> {
    run(worktree, &["add", "-A"])?;
    if run(worktree, &["status", "--porcelain"])?.trim().is_empty() { return Ok(()); }
    run(worktree, &["-c", "user.name=Retinue", "-c", "user.email=retinue@localhost", "commit", "-q", "-m", message])
        .map(|_| ())
}

/// Only a copy inside this repository's own `<repo>-retinue` family is ever merged,
/// discarded or restored, so a forged record cannot name any other folder.
fn in_family(repo: &Path, worktree: &Path) -> bool {
    let family = repo.file_name().map(|name| format!("{}{FAMILY_SUFFIX}", name.to_string_lossy().to_lowercase()));
    let parent = worktree.parent().and_then(|parent| parent.file_name()).map(|name| name.to_string_lossy().to_lowercase());
    family.is_some() && parent == family
}

/// Whether git lists this folder as one of the repository's worktrees.
fn is_registered(repo: &Path, worktree: &Path) -> Result<bool, String> {
    let wanted = normalize(worktree);
    Ok(run(repo, &["worktree", "list", "--porcelain"])?.lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .any(|path| normalize(Path::new(path.trim())) == wanted))
}

/// True when the folder is gone or holds nothing. A folder that can't be read is neither.
pub(crate) fn is_empty_or_missing(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Whether the copy is a registered linked worktree of `repo` with `branch`
/// checked out. A merge refuses anything else, so a forged proposal payload
/// cannot point at an arbitrary branch or folder.
fn verify(repo: &Path, worktree: &Path, branch: &str) -> Result<(), String> {
    if !branch.starts_with("retinue/") { return Err("That is not a Retinue task branch.".to_owned()); }
    if !worktree.is_dir() { return Err("The task's separate copy is gone.".to_owned()); }
    if !in_family(repo, worktree) { return Err("That copy is not in this repository's task folder.".to_owned()); }
    if !is_registered(repo, worktree)? { return Err("That copy is not a worktree of this repository.".to_owned()); }
    let head = run(worktree, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if head != branch { return Err("The task branch is not checked out in that copy.".to_owned()); }
    Ok(())
}

/// Merge a task branch back into the main checkout: fast-forward when possible,
/// otherwise a merge commit. Never forced. A dirty or mis-branched main checkout
/// and any conflict are refused with nothing changed.
pub(crate) fn merge(repo: &Path, worktree: &Path, branch: &str, base: &str) -> Result<String, String> {
    let repo = repo_root(repo).ok_or_else(|| "The project folder is no longer a git repository.".to_owned())?;
    verify(&repo, worktree, branch)?;
    if is_dirty(&repo)? {
        // The copy exists because another task was working here, so a dirty
        // checkout is often that task's own work in progress — never tell the
        // owner to stash it. Untracked files count as dirty too.
        return Err("Another task may still be working in the project folder, or it has uncommitted changes (including new files). Merge once that work is saved and the folder is clean, then approve again.".to_owned());
    }
    let current = current_branch(&repo).map_err(|_| "The project folder is not on a branch, so it cannot merge.".to_owned())?;
    if current != base {
        return Err(format!("The project folder is on {current}, not {base}. Switch back to {base}, then approve again."));
    }
    if run(&repo, &["merge-base", "--is-ancestor", branch, "HEAD"]).is_ok() { return Ok("already merged".to_owned()); }
    if run(&repo, &["merge", "--ff-only", branch]).is_ok() { return Ok("fast-forward".to_owned()); }
    // A merge commit needs an author. Use the same fixed Retinue identity as the
    // task commit, so a machine without a global git identity can still merge.
    match run(&repo, &["-c", "user.name=Retinue", "-c", "user.email=retinue@localhost", "merge", "--no-ff", "--no-edit", branch]) {
        Ok(_) => Ok("merge commit".to_owned()),
        Err(_) => {
            let _ = run(&repo, &["merge", "--abort"]);
            Err("The changes could not be merged cleanly. The branch and its separate copy are untouched.".to_owned())
        }
    }
}

/// Take a task copy away. Returns whether it is gone. On Windows git deletes the
/// files and its record, then fails on the folder itself while a process, such as
/// the coding agent, has it as its working folder. The copy is gone by then, and
/// the empty folder only lasts as long as that process. A folder with anything
/// left in it is a leftover, and must never be reported as a completed cleanup.
fn remove_copy(repo: &Path, worktree: &Path) -> bool {
    let path = worktree.to_string_lossy().into_owned();
    let _ = run(repo, &["worktree", "remove", "--force", &path]);
    let _ = run(repo, &["worktree", "prune"]);
    is_empty_or_missing(worktree) && !is_registered(repo, worktree).unwrap_or(true)
}

/// Remove a task copy and its now-merged branch. Returns whether the copy is gone.
pub(crate) fn remove_after_merge(repo: &Path, worktree: &Path, branch: &str) -> bool {
    if !remove_copy(repo, worktree) { return false; }
    let _ = run(repo, &["branch", "-d", branch]);
    true
}

/// Discard a task copy the owner rejected: remove the worktree and delete the
/// branch even though its work was never merged. Only reached on an explicit
/// owner choice, or for a copy no conversation ever opened. Returns whether the
/// copy is gone.
pub(crate) fn discard(repo: &Path, worktree: &Path, branch: &str) -> bool {
    // A rejected proposal can be edited before it is decided, so discard is
    // bounded by the same check as a merge: only this repo's own task copy and
    // branch are removed.
    if verify(repo, worktree, branch).is_err() { return false; }
    if !remove_copy(repo, worktree) { return false; }
    let _ = run(repo, &["branch", "-D", branch]);
    true
}

/// What a conversation's copy needed before its next turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Repair {
    Intact,
    Restored,
    Lost,
}

/// Put a task copy back when its files were taken away while its conversation was
/// still open, which versions before 31 October 2026 did after a turn that changed
/// nothing. The branch survives that, so the copy comes back as it was. Anything in
/// the folder could be the only copy of a bot's work, and git won't check out over
/// it, so a folder with files in it is left alone.
pub(crate) fn restore(repo: &Path, worktree: &Path, branch: &str, base_commit: &str) -> Repair {
    // A linked worktree always has its `.git` file, so this runs no git in the usual case.
    if worktree.join(".git").exists() { return Repair::Intact; }
    if !branch.starts_with("retinue/") || !in_family(repo, worktree) || !is_empty_or_missing(worktree) { return Repair::Lost; }
    let _ = run(repo, &["worktree", "prune"]);
    let path = worktree.to_string_lossy().into_owned();
    let added = if branch_exists(repo, branch) {
        run(repo, &["worktree", "add", &path, branch])
    } else {
        run(repo, &["worktree", "add", "-b", branch, &path, base_commit])
    };
    if added.is_ok() { Repair::Restored } else { Repair::Lost }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().map(|output| output.status.success()).unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git").current_dir(dir).args(args).output().map(|output| output.status.success()).unwrap_or(false)
    }

    /// A temp repo with one committed file, or `None` when git is not on PATH.
    fn repo() -> Option<PathBuf> {
        if !git_available() { return None; }
        let dir = std::env::temp_dir().join(format!("retinue-wt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(git(&dir, &["init", "-q"]));
        assert!(git(&dir, &["config", "core.autocrlf", "false"]));
        std::fs::write(dir.join("readme.txt"), "hello\n").unwrap();
        assert!(git(&dir, &["add", "."]));
        assert!(git(&dir, &["-c", "user.name=Test", "-c", "user.email=test@retinue.local", "commit", "-q", "-m", "init"]));
        Some(dir)
    }

    #[test]
    fn slug_is_branch_safe_and_never_empty() {
        assert_eq!(slug("Add login!"), "add-login");
        assert_eq!(slug("   "), "task");
        assert_eq!(slug("a/b\\c:d"), "a-b-c-d");
        assert!(slug(&"x".repeat(120)).len() <= MAX_SLUG);
    }

    #[test]
    fn a_taken_name_gets_a_number_and_then_a_short_suffix() {
        let base = slug("Add login");
        assert_eq!(candidate_slug(&base, 0), "add-login");
        assert_eq!(candidate_slug(&base, 1), "add-login-2");
        assert_eq!(candidate_slug(&base, 19), "add-login-20");
        // Past the numbered names a short random suffix keeps the copy inside the
        // slug limit and safe for a branch.
        let long = slug(&"the same first words every message ".repeat(4));
        assert_eq!(long.len(), MAX_SLUG);
        let suffixed = (NAME_ATTEMPTS..NAME_ATTEMPTS + 8).map(|attempt| candidate_slug(&long, attempt)).collect::<Vec<_>>();
        for name in &suffixed {
            assert!(name.len() <= MAX_SLUG, "{name}");
            assert_ne!(name, &long);
            assert!(name.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'), "{name}");
            assert!(!name.starts_with('-') && !name.ends_with('-'), "{name}");
        }
        assert!(suffixed.iter().collect::<std::collections::BTreeSet<_>>().len() > 1, "{suffixed:?}");
    }

    #[test]
    fn repo_root_is_none_for_a_plain_folder() {
        let dir = std::env::temp_dir().join(format!("retinue-plain-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(repo_root(&dir).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_family_full_of_one_name_still_gets_a_copy() {
        // Owner report (21 November 2026): two messages that start the same way
        // both want `add-login`. The numbered names can all already be taken by
        // copies kept from earlier conversations, so a later copy must still get a
        // short suffix instead of a refusal.
        let Some(repo) = repo() else { return };
        let mut made = Vec::new();
        for index in 0..NAME_ATTEMPTS {
            match create(&repo, "Add login") {
                Ok(copy) => made.push(copy),
                Err(error) => panic!("copy {} failed: {error}", index + 1),
            }
        }
        assert_eq!(made.iter().map(|copy| copy.branch.clone()).collect::<std::collections::BTreeSet<_>>().len(), NAME_ATTEMPTS, "every copy has its own name");
        let extra = create(&repo, "Add login").expect("a task copy still gets a name once every numbered one is taken");
        assert!(extra.branch.starts_with("retinue/add-login-"), "{}", extra.branch);
        assert_eq!(extra.branch.rsplit('-').next().unwrap().len(), SUFFIX_CHARS, "{}", extra.branch);
        for copy in made.iter().chain(std::iter::once(&extra)) {
            discard(&repo, &copy.path, &copy.branch);
        }
        if let Some(family) = made.first().and_then(|copy| copy.path.parent()) { let _ = std::fs::remove_dir_all(family); }
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn worktree_round_trip_merges_then_cleans_up() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        assert_eq!(created.branch, "retinue/add-login");
        // Compare against git's own toplevel (`created.repo`), because the test's
        // `temp_dir()` may be the 8.3 short form while git reports the long one.
        let family = created.repo.parent().unwrap().join(format!("{}-retinue", created.repo.file_name().unwrap().to_str().unwrap()));
        assert!(created.path.starts_with(&family));
        assert_eq!(run(&created.path, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(), "retinue/add-login");

        std::fs::write(created.path.join("feature.txt"), "one\ntwo\n").unwrap();
        commit_all(&created.path, "Retinue task add-login").unwrap();
        let summary = inspect(&created.path, &created.base_commit).unwrap();
        assert_eq!(summary.files, vec!["feature.txt".to_owned()]);
        assert_eq!(summary.insertions, 2);

        assert_eq!(merge(&repo, &created.path, &created.branch, &created.base).unwrap(), "fast-forward");
        assert_eq!(std::fs::read_to_string(repo.join("feature.txt")).unwrap(), "one\ntwo\n");
        remove_after_merge(&repo, &created.path, &created.branch);
        assert!(!created.path.exists());
        assert!(!branch_exists(&repo, &created.branch));
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn merge_refuses_a_dirty_main_checkout() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        std::fs::write(created.path.join("feature.txt"), "x\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        // The owner edits the main checkout without committing.
        std::fs::write(repo.join("readme.txt"), "changed\n").unwrap();
        let error = merge(&repo, &created.path, &created.branch, &created.base).unwrap_err();
        assert!(error.contains("uncommitted changes"), "{error}");
        assert!(!repo.join("feature.txt").exists(), "nothing merged while dirty");
        assert!(created.path.exists(), "the task copy survives a refusal");
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn a_merge_conflict_is_refused_and_leaves_main_clean() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        // Both sides change the same line of the same file.
        std::fs::write(created.path.join("readme.txt"), "task side\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        std::fs::write(repo.join("readme.txt"), "main side\n").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["-c", "user.name=Test", "-c", "user.email=test@retinue.local", "commit", "-q", "-m", "main edit"]));
        let error = merge(&repo, &created.path, &created.branch, &created.base).unwrap_err();
        assert!(error.contains("could not be merged"), "{error}");
        // Nothing is left half-merged: no merge in progress, the main edit is
        // untouched, and the task copy survives.
        assert!(!repo.join(".git").join("MERGE_HEAD").exists(), "no merge is left in progress");
        assert_eq!(std::fs::read_to_string(repo.join("readme.txt")).unwrap(), "main side\n");
        assert!(created.path.exists(), "the task copy survives a conflict");
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn a_diverged_history_merges_with_a_merge_commit() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        std::fs::write(created.path.join("feature.txt"), "task\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        // A non-conflicting commit on main, so a fast-forward is impossible.
        std::fs::write(repo.join("other.txt"), "main\n").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["-c", "user.name=Test", "-c", "user.email=test@retinue.local", "commit", "-q", "-m", "main edit"]));
        assert_eq!(merge(&repo, &created.path, &created.branch, &created.base).unwrap(), "merge commit");
        assert!(repo.join("feature.txt").exists() && repo.join("other.txt").exists());
        assert!(!repo.join(".git").join("MERGE_HEAD").exists());
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn an_already_merged_branch_is_reported_not_remerged() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        std::fs::write(created.path.join("feature.txt"), "x\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        assert_eq!(merge(&repo, &created.path, &created.branch, &created.base).unwrap(), "fast-forward");
        // The branch is now an ancestor of HEAD, so a second approval is a no-op.
        assert_eq!(merge(&repo, &created.path, &created.branch, &created.base).unwrap(), "already merged");
        assert!(remove_after_merge(&repo, &created.path, &created.branch), "the copy is gone");
        assert!(!branch_exists(&repo, &created.branch));
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn discard_removes_the_copy_and_branch() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Throwaway").unwrap();
        std::fs::write(created.path.join("scratch.txt"), "x\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        discard(&repo, &created.path, &created.branch);
        assert!(!created.path.exists());
        assert!(!branch_exists(&repo, &created.branch));
        std::fs::remove_dir_all(&repo).ok();
    }

    /// The folder as a process's working folder, as a coding agent has it.
    #[cfg(windows)]
    fn holder(folder: &Path) -> std::process::Child {
        Command::new("ping").args(["-n", "60", "127.0.0.1"]).stdout(std::process::Stdio::null()).current_dir(folder).spawn().unwrap()
    }

    // What Windows really does: git deletes everything, then fails on the folder itself.
    // The work is gone and git's record with it, so that is a removal, not a failure.
    #[cfg(windows)]
    #[test]
    fn a_folder_only_held_as_a_working_folder_is_emptied_and_counts_as_gone() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Held").unwrap();
        std::fs::write(created.path.join("scratch.txt"), "x\n").unwrap();
        commit_all(&created.path, "Retinue task").unwrap();
        let mut held = holder(&created.path);
        let gone = discard(&repo, &created.path, &created.branch);
        let contents = std::fs::read_dir(&created.path).map(|entries| entries.count());
        let listed = run(&repo, &["worktree", "list", "--porcelain"]).unwrap();
        let kept_branch = branch_exists(&repo, &created.branch);
        let _ = held.kill();
        let _ = held.wait();
        assert_eq!(contents.unwrap(), 0, "git deleted everything inside the folder");
        assert!(!listed.lines().filter_map(|line| line.strip_prefix("worktree ")).any(|path| normalize(Path::new(path)) == normalize(&created.path)), "and its record: {listed}");
        assert!(gone, "so the copy is gone");
        assert!(!kept_branch, "and its branch goes too");
        std::fs::remove_dir_all(&created.path).ok();
        std::fs::remove_dir_all(&repo).ok();
    }

    #[cfg(windows)]
    #[test]
    fn a_file_stuck_in_the_copy_is_not_a_removal() {
        use std::os::windows::fs::OpenOptionsExt as _;
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Stuck").unwrap();
        // No sharing at all, so nothing can delete this file while it's open.
        let stuck = std::fs::OpenOptions::new().read(true).share_mode(0).open(created.path.join("readme.txt")).unwrap();
        assert!(!discard(&repo, &created.path, &created.branch), "something is left");
        assert!(branch_exists(&repo, &created.branch), "so the branch stays");
        drop(stuck);
        std::fs::remove_dir_all(&created.path).ok();
        std::fs::remove_dir_all(&repo).ok();
    }

    #[test]
    fn a_copy_outside_the_task_folder_is_refused() {
        let Some(repo) = repo() else { return };
        let outside = std::env::temp_dir().join(format!("retinue-outside-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).unwrap();
        let branch = "retinue/outside";
        assert!(git(&repo, &["worktree", "add", "-b", branch, outside.to_str().unwrap(), "HEAD"]));
        assert!(merge(&repo, &outside, branch, &current_branch(&repo).unwrap()).is_err(), "a copy outside <repo>-retinue is never merged");
        discard(&repo, &outside, branch);
        assert!(outside.exists(), "discard refuses a copy outside the task folder");
        // Clean up the deliberately-created worktree.
        let _ = run(&repo, &["worktree", "remove", "--force", outside.to_str().unwrap()]);
        let _ = run(&repo, &["branch", "-D", branch]);
        std::fs::remove_dir_all(&repo).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn a_forged_branch_is_refused() {
        let Some(repo) = repo() else { return };
        let created = create(&repo, "Add login").unwrap();
        assert!(merge(&repo, &created.path, "main", &created.base).is_err());
        assert!(merge(&repo, &created.path, "retinue/other", &created.base).is_err());
        discard(&repo, &created.path, &created.branch);
        std::fs::remove_dir_all(&repo).ok();
    }
}
