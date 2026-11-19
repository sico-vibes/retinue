//! Gives `retinue-hook.exe` a Windows version resource.
//!
//! The hook is copied into the installer beside the app, and a file with no
//! publisher, product or version is what the Defender heuristics score as
//! unknown (owner report, 19 November 2026: `beta.51` was named
//! `Program:Win32/Contebrew.A!ml`). A resource signs nothing; it says who the
//! file is, which is the cheap half of the problem. See
//! `docs/operations/code-signing.md`.

use std::{env, fs, path::PathBuf};

fn main() {
    write_version_resource();
}

/// The version resource Windows shows on the file's Details tab.
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
VALUE "FileDescription", "Retinue hook bridge"
VALUE "FileVersion", "{version}"
VALUE "InternalName", "retinue-hook"
VALUE "LegalCopyright", "Copyright (C) 2026 Bassem Hesham"
VALUE "OriginalFilename", "retinue-hook.exe"
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
    let path = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("retinue-hook.rc");
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
