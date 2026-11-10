//! Embeds `skill_library/` into the daemon binary.
//!
//! The library is a folder per skill (`skill_library/<id>/SKILL.md` and its
//! Markdown references). This script writes `$OUT_DIR/skill_library_files.rs`: one
//! static table of `(id, [(relative path, bytes)])`, every file pulled in with
//! `include_bytes!`, so the installed daemon needs no folder beside it and no
//! new dependency.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.is_file() {
            let rel = path.strip_prefix(root).expect("inside the skill folder").to_string_lossy().replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=skill_library");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let library = manifest.join("skill_library");
    let mut ids: Vec<String> = fs::read_dir(&library)
        .map(|entries| entries.flatten().filter(|entry| entry.path().is_dir()).filter_map(|entry| entry.file_name().into_string().ok()).filter(|name| name != "licenses").collect())
        .unwrap_or_default();
    ids.sort();
    let mut code = String::from("pub type LibraryFile = (&'static str, &'static [u8]);\npub type LibrarySkillFiles = (&'static str, &'static [LibraryFile]);\npub static LIBRARY_FILES: &[LibrarySkillFiles] = &[\n");
    for id in ids {
        let root = library.join(&id);
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        files.sort_by(|a, b| a.0.cmp(&b.0));
        code.push_str(&format!("    ({id:?}, &[\n"));
        for (rel, path) in files {
            code.push_str(&format!("        ({rel:?}, include_bytes!({:?})),\n", path.to_string_lossy()));
        }
        code.push_str("    ]),\n");
    }
    code.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("skill_library_files.rs");
    fs::write(out, code).expect("write the skill library table");
}
