//! Embeds `skill_library/` into the daemon binary, and gives it a Windows
//! version resource.
//!
//! The library is a folder per skill (`skill_library/<id>/SKILL.md` and its
//! Markdown references). This script writes `$OUT_DIR/skill_library_files.rs`: one
//! static table of `(id, [(relative path, bytes)])`, every file pulled in with
//! `include_bytes!`, so the installed daemon needs no folder beside it and no
//! new dependency.
//!
//! The version resource is the other half: `retinued.exe` ships inside the
//! installer, and a file with no publisher, product or version is what the
//! Defender heuristics score as unknown (see `docs/operations/code-signing.md`).

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
    write_skill_library();
    write_version_resource();
}

/// The table of embedded skill files.
fn write_skill_library() {
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

/// The version resource Windows shows on the file's Details tab.
///
/// A sidecar with no publisher, product or version is the shape the Defender
/// heuristics are tuned for (owner report, 19 November 2026: `beta.51` was named
/// `Program:Win32/Contebrew.A!ml`). A resource signs nothing; it says who the
/// file is, which is the cheap half of the problem.
fn write_version_resource() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let [a, b, c, d] = windows_version(&version);
    let resource = format!(
        r#"1 VERSIONINFO
FILEVERSION {a}, {b}, {c}, {d}
PRODUCTVERSION {a}, {b}, {c}, {d}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
{{
BLOCK "StringFileInfo"
{{
BLOCK "040904b0"
{{
VALUE "CompanyName", "Bassem Hesham"
VALUE "FileDescription", "Retinue daemon"
VALUE "FileVersion", "{version}"
VALUE "InternalName", "retinued"
VALUE "LegalCopyright", "Copyright (C) 2026 Bassem Hesham"
VALUE "OriginalFilename", "retinued.exe"
VALUE "ProductName", "Retinue"
VALUE "ProductVersion", "{version}"
}}
}}
BLOCK "VarFileInfo" {{
VALUE "Translation", 0x409, 0x04b0
}}
}}
"#
    );
    let path = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("retinued.rc");
    fs::write(&path, resource).expect("write the version resource");
    println!("cargo:rerun-if-changed=build.rs");
    embed_resource::compile(&path, embed_resource::NONE)
        .manifest_optional()
        .expect("compile the version resource");
}

/// The four numbers Windows stores beside the version string. `0.2.0-beta.5`
/// cannot be one number, so the beta counter becomes the build number.
fn windows_version(version: &str) -> [u16; 4] {
    let mut numbers = [0u16; 4];
    for (slot, part) in numbers.iter_mut().zip(version.split('.')) {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        *slot = digits.parse().unwrap_or(0);
    }
    numbers
}
