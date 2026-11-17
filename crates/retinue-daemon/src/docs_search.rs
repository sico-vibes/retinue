//! The documentation library, searchable offline by bots (wave 2, nod idea 6).
//!
//! Bots answer "how do I..." questions about Retinue from their own instructions
//! and get details wrong. The app already ships a documentation library
//! (`docs/library/content`, the sources of `docs/DOCUMENTATION.html`), so this
//! module embeds those Markdown pages in the daemon binary with `include_str!`
//! and serves two tools over them: `search_docs` finds a page and a line,
//! `read_doc` reads a bounded slice of it.
//!
//! `include_str!` rather than a generated table in `build.rs` on purpose: the
//! daemon's build script already exists for the vendored skill library, and a
//! second generated table would rebuild the daemon for a documentation edit that
//! has nothing to do with the daemon. The list below is one line per page, and
//! `the_embedded_pages_match_the_library` fails when a page is added to the
//! library without a line here.
//!
//! The rails:
//!
//! - Offline. Nothing leaves the machine and no network is used.
//! - Bounded. A search returns at most 5 hits, a read at most 200 lines from a
//!   chosen offset, a query is at most 200 characters, and a path must name an
//!   embedded page.
//! - Honest. The tools say the docs describe shipped behaviour, not the state of
//!   this machine, and every answer carries the path so a bot can cite it.

use serde_json::{json, Value};

/// One embedded documentation page: its path under `docs/library/content`.
pub struct DocFile {
    pub path: &'static str,
    pub text: &'static str,
}

/// Every page of the library, sorted by path.
pub static DOC_FILES: &[DocFile] = &[
    DocFile { path: "glossary.md", text: include_str!("../../../docs/library/content/glossary.md") },
    DocFile { path: "overview.md", text: include_str!("../../../docs/library/content/overview.md") },
    DocFile { path: "guides/automations-memory-and-skills.md", text: include_str!("../../../docs/library/content/guides/automations-memory-and-skills.md") },
    DocFile { path: "guides/backup-updates-and-troubleshooting.md", text: include_str!("../../../docs/library/content/guides/backup-updates-and-troubleshooting.md") },
    DocFile { path: "guides/bots.md", text: include_str!("../../../docs/library/content/guides/bots.md") },
    DocFile { path: "guides/companion-and-dictation.md", text: include_str!("../../../docs/library/content/guides/companion-and-dictation.md") },
    DocFile { path: "guides/conversations-and-approvals.md", text: include_str!("../../../docs/library/content/guides/conversations-and-approvals.md") },
    DocFile { path: "guides/expansion-handoff.md", text: include_str!("../../../docs/library/content/guides/expansion-handoff.md") },
    DocFile { path: "guides/five-minute-path.md", text: include_str!("../../../docs/library/content/guides/five-minute-path.md") },
    DocFile { path: "guides/looks-and-appearance.md", text: include_str!("../../../docs/library/content/guides/looks-and-appearance.md") },
    DocFile { path: "guides/remote-access.md", text: include_str!("../../../docs/library/content/guides/remote-access.md") },
    DocFile { path: "guides/tasks.md", text: include_str!("../../../docs/library/content/guides/tasks.md") },
    DocFile { path: "guides/team-and-manager.md", text: include_str!("../../../docs/library/content/guides/team-and-manager.md") },
    DocFile { path: "reference/approvals-and-permissions.md", text: include_str!("../../../docs/library/content/reference/approvals-and-permissions.md") },
    DocFile { path: "reference/architecture.md", text: include_str!("../../../docs/library/content/reference/architecture.md") },
    DocFile { path: "reference/bot-art-system.md", text: include_str!("../../../docs/library/content/reference/bot-art-system.md") },
    DocFile { path: "reference/daemon-and-storage.md", text: include_str!("../../../docs/library/content/reference/daemon-and-storage.md") },
    DocFile { path: "reference/development-and-testing.md", text: include_str!("../../../docs/library/content/reference/development-and-testing.md") },
    DocFile { path: "reference/ipc-and-events.md", text: include_str!("../../../docs/library/content/reference/ipc-and-events.md") },
    DocFile { path: "reference/live-development-review-2026-10-20.md", text: include_str!("../../../docs/library/content/reference/live-development-review-2026-10-20.md") },
    DocFile { path: "reference/release-and-updates.md", text: include_str!("../../../docs/library/content/reference/release-and-updates.md") },
    DocFile { path: "reference/remote-gateway.md", text: include_str!("../../../docs/library/content/reference/remote-gateway.md") },
    DocFile { path: "reference/runtimes-and-adapters.md", text: include_str!("../../../docs/library/content/reference/runtimes-and-adapters.md") },
    DocFile { path: "reference/sessions-and-turns.md", text: include_str!("../../../docs/library/content/reference/sessions-and-turns.md") },
    DocFile { path: "reference/settings.md", text: include_str!("../../../docs/library/content/reference/settings.md") },
    DocFile { path: "reference/task-board.md", text: include_str!("../../../docs/library/content/reference/task-board.md") },
    DocFile { path: "reference/theme-system.md", text: include_str!("../../../docs/library/content/reference/theme-system.md") },
];

/// Longest query the tools accept, in characters.
pub const MAX_QUERY_CHARS: usize = 200;
/// Most hits one search returns.
pub const MAX_HITS: usize = 5;
/// Most lines one read returns.
pub const MAX_READ_LINES: usize = 200;

/// Every embedded page, by path.
fn files() -> &'static [DocFile] {
    DOC_FILES
}

/// The page's title: its first Markdown heading, or the file name.
fn title_of(text: &str, path: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md").to_owned())
}

/// Accepts `reference/settings.md`, a leading slash, backslashes, or the full
/// repository path, and answers with the embedded path when one matches.
fn normalize_path(raw: &str) -> Option<&'static str> {
    let cleaned = raw.trim().replace('\\', "/");
    let cleaned = cleaned.trim_start_matches('/');
    let cleaned = cleaned.rsplit("content/").next().unwrap_or(cleaned);
    files().iter().map(|file| file.path).find(|path| path.eq_ignore_ascii_case(cleaned))
}

/// Query terms: lower-case alphanumeric words of two characters or more.
fn terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in query.to_lowercase().split(|ch: char| !ch.is_alphanumeric()) {
        if word.chars().count() < 2 || out.iter().any(|seen| seen == word) { continue; }
        out.push(word.to_owned());
        if out.len() >= 8 { break; }
    }
    out
}

fn count_terms(haystack: &str, terms: &[String]) -> usize {
    let lower = haystack.to_lowercase();
    terms.iter().filter(|term| lower.contains(term.as_str())).count()
}

struct Hit {
    score: usize,
    path: &'static str,
    title: String,
    line_number: usize,
    line: String,
}

/// `search_docs`: the pages that best match, each with one matching line.
pub fn search(query: &str) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() { return Err("query is required.".into()); }
    if query.chars().count() > MAX_QUERY_CHARS { return Err(format!("query must be {MAX_QUERY_CHARS} characters or fewer.")); }
    let terms = terms(query);
    if terms.is_empty() { return Err("query must contain a word of two characters or more.".into()); }
    let mut hits: Vec<Hit> = Vec::new();
    for file in files() {
        let text = file.text;
        let title = title_of(text, file.path);
        // A page that matches nothing scores zero, so it is left out entirely.
        let page_terms = count_terms(text, &terms);
        if page_terms == 0 { continue; }
        // The best line: the first one that carries the most terms.
        let mut best: Option<(usize, usize, String)> = None;
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() { continue; }
            let found = count_terms(line, &terms);
            if found == 0 { continue; }
            let better = best.as_ref().is_none_or(|(count, _, _)| found > *count);
            if better { best = Some((found, index + 1, line.trim().to_owned())); }
            if found == terms.len() { break; }
        }
        let Some((line_terms, line_number, line)) = best else { continue };
        let title_bonus = count_terms(&title, &terms) * 3;
        let score = page_terms + title_bonus + line_terms * 2 + usize::from(page_terms == terms.len()) * 4;
        hits.push(Hit { score, path: file.path, title, line_number, line: clip(&line, 240) });
    }
    if hits.is_empty() { return Ok(format!("No documentation page matches \"{query}\".")); }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(b.path)));
    hits.truncate(MAX_HITS);
    let rows = hits.iter().map(|hit| format!("- {} · {} · line {}: {}", hit.path, hit.title, hit.line_number, hit.line)).collect::<Vec<_>>();
    Ok(format!(
        "Documentation pages matching \"{query}\" (these describe shipped behaviour, not the state of this machine; cite the path, and read one with read_doc):\n\n{}",
        rows.join("\n")
    ))
}

/// `read_doc`: up to 200 lines of one page from `offset`.
pub fn read(path: &str, offset: usize) -> Result<String, String> {
    let Some(path) = normalize_path(path) else {
        return Err("No documentation page has that path. Use search_docs first.".into());
    };
    let Some(file) = files().iter().find(|file| file.path == path) else {
        return Err("No documentation page has that path. Use search_docs first.".into());
    };
    let text = file.text;
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let start = offset.min(total);
    let end = (start + MAX_READ_LINES).min(total);
    let body = lines[start..end].join("\n");
    let more = if end < total { format!("\n\n[{} more lines; read again with offset {}]", total - end, end) } else { String::new() };
    Ok(format!("{path} · {total} lines · lines {} to {end}\n\n{body}{more}", start + 1))
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() > limit { text.chars().take(limit - 1).collect::<String>() + "…" } else { text.to_owned() }
}

/// The two MCP tool schemas.
pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "name": "search_docs",
            "description": "Search Retinue's own documentation (offline, the pages the app ships) for a question about how Retinue works, a setting, a tool, a screen or a rule. Cite the page path in your answer. The pages describe shipped behaviour, not the state of this machine.",
            "inputSchema": {"type": "object", "properties": {"query": {"type": "string", "description": "Words to look for, up to 200 characters."}}, "required": ["query"], "additionalProperties": false}
        }),
        json!({
            "name": "read_doc",
            "description": "Read up to 200 lines of one documentation page, from a line offset. Use the path search_docs returned.",
            "inputSchema": {"type": "object", "properties": {"path": {"type": "string", "description": "The page path from search_docs, such as reference/settings.md."}, "offset": {"type": "integer", "minimum": 0, "description": "The line to start at (default 0)."}}, "required": ["path"], "additionalProperties": false}
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list above is hand-kept; this is what keeps it honest when a page is
    /// added to the library. It reads the repository's own folder, which exists
    /// wherever the tests run.
    #[test]
    fn the_embedded_pages_match_the_library() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/library/content"));
        let mut on_disk = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() { stack.push(path); continue; }
                if path.extension().and_then(|ext| ext.to_str()) != Some("md") { continue; }
                let rel = path.strip_prefix(root).expect("inside the library").to_string_lossy().replace('\\', "/");
                on_disk.push(rel);
            }
        }
        on_disk.sort();
        let mut embedded = files().iter().map(|file| file.path.to_owned()).collect::<Vec<_>>();
        embedded.sort();
        assert_eq!(embedded, on_disk, "add a DocFile line for every page in docs/library/content");
    }

    #[test]
    fn a_search_names_a_page_and_a_line_and_an_empty_one_says_so() {
        let found = search("approval").expect("a search");
        assert!(found.contains(".md · "), "{found}");
        assert!(found.contains("line "), "{found}");
        assert!(found.contains("read_doc"), "{found}");
        assert!(search("zzzznotawordzzzz").unwrap().contains("No documentation page matches"));
        assert!(search("   ").is_err());
    }

    #[test]
    fn a_read_is_bounded_and_an_unknown_path_is_refused() {
        let first = read("reference/settings.md", 0).expect("a read");
        assert!(first.contains("reference/settings.md · "), "{first}");
        let deep = read("reference/settings.md", 100_000).expect("a read past the end");
        assert!(deep.lines().count() <= 4, "{deep}");
        assert!(read("reference/nowhere.md", 0).is_err());
        // A path in any of the shapes a bot might copy still resolves.
        assert!(read("docs/library/content/reference/settings.md", 0).is_ok());
        assert!(read("reference\\settings.md", 0).is_ok());
    }
}
