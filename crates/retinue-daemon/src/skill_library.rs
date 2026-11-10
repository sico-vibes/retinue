//! The vendored skill library (owner decision, 9 November 2026).
//!
//! Retinue ships a curated set of open-source skills inside the daemon binary
//! (`skill_library/`, embedded by `build.rs`; sources, commits and licences are
//! in `skill_library/SOURCES.md`). A preset employee names the library skills it
//! wears by default and the optional ones. Hiring installs the chosen ones into
//! the shared pool (`skills.rs`) with the same guards an import has, and
//! switches them on for that employee only.
//!
//! A library skill belongs to the employees that chose it: when one is newly
//! installed it is switched off for every other bot, and the new employee gets
//! every installed library skill it did not choose switched off. The owner can
//! still turn any of them on or off in Settings.

use crate::{skills, AppState};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/skill_library_files.rs"));

const INDEX_JSON: &str = include_str!("../skill_library/index.json");

/// Most library skills one hire may install.
pub const MAX_HIRE_SKILLS: usize = 15;

/// One library skill's card, from `skill_library/index.json`.
#[derive(Deserialize, Clone)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub licence: String,
    pub author: String,
    pub url: String,
    pub commit: String,
}

fn index() -> &'static Vec<Entry> {
    static INDEX: OnceLock<Vec<Entry>> = OnceLock::new();
    INDEX.get_or_init(|| serde_json::from_str(INDEX_JSON).expect("skill_library/index.json is valid at build time"))
}

/// Every library skill that is both listed and embedded, in index order.
pub fn entries() -> impl Iterator<Item = &'static Entry> {
    index().iter().filter(|entry| files_of(&entry.id).is_some())
}

pub fn get(id: &str) -> Option<&'static Entry> {
    entries().find(|entry| entry.id == id)
}

fn files_of(id: &str) -> Option<&'static [LibraryFile]> {
    LIBRARY_FILES.iter().find(|(folder, _)| *folder == id).map(|(_, files)| *files)
}

/// `{id, name, description, licence, author, url, commit}` for `preset.list`'s
/// `skillLibrary`: where each skill came from, pinned to a commit.
pub fn list() -> Vec<Value> {
    entries().map(|entry| json!({"id": entry.id, "name": entry.name, "description": entry.description, "licence": entry.licence, "author": entry.author, "url": entry.url, "commit": entry.commit})).collect()
}

/// `{id, name, description}` for one id, or null when the library has no such skill.
pub fn card(id: &str) -> Value {
    get(id).map(|entry| json!({"id": entry.id, "name": entry.name, "description": entry.description})).unwrap_or(Value::Null)
}

/// The files of one library skill, checked the way an import is: a `SKILL.md`
/// whose frontmatter name is the id, safe relative paths, and the same caps.
type OwnedFiles = Vec<(String, Vec<u8>)>;

fn checked_files(id: &str) -> Result<(OwnedFiles, String), String> {
    let files = files_of(id).ok_or_else(|| format!("There is no library skill called {id}."))?;
    if files.len() > skills::MAX_FILES {
        return Err(format!("The library skill {id} has too many files."));
    }
    let total: u64 = files.iter().map(|(_, bytes)| bytes.len() as u64).sum();
    if total > skills::MAX_BYTES {
        return Err(format!("The library skill {id} is too large."));
    }
    let mut out = Vec::with_capacity(files.len());
    for (rel, bytes) in files {
        if !skills::safe_relative(rel) {
            return Err(format!("The library skill {id} has an unsupported path: {rel}"));
        }
        out.push(((*rel).to_owned(), bytes.to_vec()));
    }
    let skill_md = out.iter().find(|(rel, _)| rel == "SKILL.md").ok_or_else(|| format!("The library skill {id} has no SKILL.md."))?;
    let text = std::str::from_utf8(&skill_md.1).map_err(|_| format!("The library skill {id} is not valid UTF-8."))?;
    let front = skills::parse_frontmatter(text)?;
    if front.name != id {
        return Err(format!("The library skill {id} names itself {}.", front.name));
    }
    Ok((out, front.description))
}

/// Installs library skills into the shared pool. A skill whose files are
/// already installed unchanged is left alone; a skill the owner already has
/// under the same name with other content is never overwritten. Returns the
/// ids that are in the pool from the library after the call (new and
/// unchanged), in the order asked. Newly installed skills are switched off for
/// every existing bot.
pub async fn install(st: &AppState, ids: &[String]) -> Result<Vec<String>, String> {
    // The pool lives in the daemon's data folder. Without one (a daemon run
    // with no `--data-dir`, and most tests) nothing is installed rather than
    // writing into the system temp folder.
    if st.team.data_dir.lock().map(|dir| dir.is_none()).unwrap_or(true) {
        return Ok(Vec::new());
    }
    let mut present = Vec::new();
    let mut fresh = Vec::new();
    for id in ids {
        if present.contains(id) {
            continue;
        }
        get(id).ok_or_else(|| format!("There is no library skill called {id}."))?;
        let (files, description) = checked_files(id)?;
        let hash = skills::files_sha256(&files);
        if let Ok(row) = st.db.skills_get(id) {
            if row["contentSha256"].as_str() == Some(hash.as_str()) {
                present.push(id.clone());
            }
            continue;
        }
        let staging = skills::stage_files(st, &files)?;
        if let Err(error) = skills::commit_staged(st, &staging, id) {
            skills::discard_staged(&staging);
            return Err(error);
        }
        let total: u64 = files.iter().map(|(_, bytes)| bytes.len() as u64).sum();
        st.db
            .skills_upsert(&json!({
                "name": id,
                "description": description,
                "source": "folder",
                "sourcePath": format!("library:{id}"),
                "contentSha256": hash,
                "fileCount": files.len() as i64,
                "totalBytes": total as i64,
            }))
            .map_err(|error| error.to_string())?;
        present.push(id.clone());
        fresh.push(id.clone());
    }
    if !fresh.is_empty() {
        let others = st.db.agent_list(true, None).map_err(|error| error.to_string())?;
        for id in &fresh {
            for agent in &others {
                if let Some(agent_id) = agent["id"].as_str() {
                    st.db.skills_toggle(id, false, Some(agent_id)).map_err(|error| error.to_string())?;
                }
            }
            st.emit("skills.changed", json!({"action": "imported", "name": id})).await;
        }
    }
    Ok(present)
}

/// Gives `agent_id` its skills: each id in `selected` is switched on for it, and
/// every other library skill that is installed is switched off for it.
pub async fn attach(st: &AppState, agent_id: &str, selected: &[String]) -> Result<(), String> {
    let installed = st.db.skills_list().map_err(|error| error.to_string())?;
    let present = |name: &str| installed.iter().any(|row| row["name"].as_str() == Some(name));
    let mut touched = Vec::new();
    for entry in entries() {
        if !present(&entry.id) {
            continue;
        }
        let on = selected.iter().any(|id| id == &entry.id);
        st.db.skills_toggle(&entry.id, on, Some(agent_id)).map_err(|error| error.to_string())?;
        touched.push(entry.id.clone());
    }
    for name in touched {
        st.emit("skills.changed", json!({"action": "toggled", "name": name})).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_library_skill_is_listed_embedded_and_valid() {
        let listed: Vec<&str> = index().iter().map(|entry| entry.id.as_str()).collect();
        let embedded: Vec<&str> = LIBRARY_FILES.iter().map(|(id, _)| *id).collect();
        for id in &listed {
            assert!(embedded.contains(id), "{id} is in index.json but not embedded");
        }
        for id in &embedded {
            assert!(listed.contains(id), "{id} is embedded but not in index.json");
        }
        assert!(listed.len() >= 20, "the library keeps a useful number of skills");
        for entry in index() {
            let (files, _) = checked_files(&entry.id).unwrap_or_else(|error| panic!("{}: {error}", entry.id));
            assert!(files.iter().all(|(rel, _)| rel.ends_with(".md")), "{} ships only Markdown", entry.id);
            assert!(matches!(entry.licence.as_str(), "MIT" | "Apache-2.0"), "{} has licence {}", entry.id, entry.licence);
            assert!(entry.commit.len() == 40 && entry.url.contains(&entry.commit), "{} is pinned to a commit", entry.id);
        }
    }
}
