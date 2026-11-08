//! The few things this window does to the computer outside the daemon: reading
//! an image from the clipboard, staging an attachment for the daemon to send,
//! opening a file or a link, and decoding the data URLs `message.image` answers
//! with.
//!
//! Each one is a port of a `src-tauri` command, so the checks are the same ones
//! the React app relies on: an image is written from its signature, a file is
//! opened only after `canonical_project_file`, and a URL is opened only when
//! `markdown::safe_http_url` accepts it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Haze's own limit: an image that would not fit the window's memory is refused
/// rather than decoded.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// How long a staged image is kept, so a folder does not grow for ever.
const STAGED_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

/// The extension an image's own bytes call for.
pub fn attachment_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

/// The image format a set of bytes is, for GPUI's decoder.
pub fn image_format(bytes: &[u8]) -> Option<gpui::ImageFormat> {
    match attachment_extension(bytes) {
        Some("png") => Some(gpui::ImageFormat::Png),
        Some("jpg") => Some(gpui::ImageFormat::Jpeg),
        Some("gif") => Some(gpui::ImageFormat::Gif),
        Some("webp") => Some(gpui::ImageFormat::Webp),
        _ => None,
    }
}

/// Write one image into the window's attachment folder and answer its path. The
/// daemon hands the provider a path, so nothing is read from the file here.
pub fn stage_attachment(folder: &Path, bytes: &[u8], name: &str) -> Result<PathBuf, String> {
    if bytes.is_empty() {
        return Err("That image is empty.".to_owned());
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err("Images must be 10 MB or smaller.".to_owned());
    }
    let extension = attachment_extension(bytes)
        .ok_or_else(|| "Only PNG, JPEG, GIF and WebP images can be attached.".to_owned())?;
    std::fs::create_dir_all(folder).map_err(|_| "The attachment folder could not be created.".to_owned())?;
    prune_staged(folder);
    let stem = clean_stem(name);
    let path = folder.join(format!("{}-{}.{extension}", stem, uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).map_err(|_| "The image could not be saved for sending.".to_owned())?;
    Ok(path)
}

/// A short, safe file stem from the name the owner's clipboard gave the image.
fn clean_stem(name: &str) -> String {
    let stem = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim_end_matches(['.', ' '])
        .replace([' ', ':', '*', '?', '"', '<', '>', '|', '.'], "-");
    let stem: String = stem.chars().filter(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_').take(40).collect();
    if stem.is_empty() {
        "attachment".to_owned()
    } else {
        stem
    }
}

/// Staged images are only needed until the provider has read them; old ones are
/// removed when a new image is staged, exactly as `prune_staged_attachments`.
fn prune_staged(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .ok()
            .filter(|metadata| metadata.is_file())
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age.as_secs() > STAGED_RETENTION_SECS);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Turn what the clipboard holds into PNG bytes, which is what every provider
/// takes. Only ever called from an explicit paste, never in the background.
///
/// GPUI's own reader is the source: Windows gives it a PNG when the source app
/// put one there, and a DIB that GPUI wraps in a BMP otherwise, so both are
/// re-encoded here (`src-tauri/src/clipboard.rs` reaches the same result).
pub fn clipboard_png(item: Option<gpui::ClipboardItem>) -> Result<Vec<u8>, String> {
    use gpui::ClipboardEntry;

    let item = item.ok_or_else(|| "The clipboard does not contain a readable image.".to_owned())?;
    for entry in item.into_entries() {
        match entry {
            ClipboardEntry::Image(image) => {
                if image.bytes.is_empty() {
                    continue;
                }
                if matches!(image.format, gpui::ImageFormat::Png) {
                    if image.bytes.len() > MAX_ATTACHMENT_BYTES {
                        return Err("The clipboard image is too large to attach.".to_owned());
                    }
                    return Ok(image.bytes);
                }
                let decoded = image::load_from_memory(&image.bytes)
                    .map_err(|_| "The clipboard image could not be decoded.".to_owned())?;
                if decoded.width() as u64 * decoded.height() as u64 > 16_777_216 {
                    return Err("The clipboard image is too large to attach.".to_owned());
                }
                let mut png = std::io::Cursor::new(Vec::new());
                decoded
                    .write_to(&mut png, image::ImageFormat::Png)
                    .map_err(|_| "The clipboard image could not be encoded.".to_owned())?;
                let bytes = png.into_inner();
                if bytes.len() > MAX_ATTACHMENT_BYTES {
                    return Err("The clipboard image is too large to attach.".to_owned());
                }
                return Ok(bytes);
            }
            // A file copied in Explorer pastes as its own picture when it is one.
            ClipboardEntry::ExternalPaths(paths) => {
                for path in paths.paths() {
                    if let Ok(bytes) = std::fs::read(path) {
                        if attachment_extension(&bytes).is_some() {
                            return Ok(bytes);
                        }
                    }
                }
            }
            // A copied picture's text is a path often enough to be worth one look.
            ClipboardEntry::String(text) => {
                let candidate = text.text().trim().trim_matches('"').to_owned();
                if !candidate.is_empty() && candidate.len() < 500 {
                    if let Ok(bytes) = std::fs::read(&candidate) {
                        if attachment_extension(&bytes).is_some() {
                            return Ok(bytes);
                        }
                    }
                }
            }
        }
    }
    Err("The clipboard does not contain a readable image.".to_owned())
}

/// The bytes of an image file the owner dropped on the window, when it is one of
/// the four kinds a provider accepts.
pub fn read_image_file(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|_| "That file could not be read.".to_owned())?;
    if attachment_extension(&bytes).is_none() {
        return Err("Only PNG, JPEG, GIF and WebP images can be attached.".to_owned());
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err("Images must be 10 MB or smaller.".to_owned());
    }
    Ok(bytes)
}

/// The file a path names, checked the way `canonical_project_file` checks it: it
/// resolves against the project folder, it really exists, it is a file, and it
/// is inside that folder.
pub fn canonical_project_file(path: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        let root = project_root.ok_or_else(|| "A project folder is required to resolve this relative file path.".to_owned())?;
        PathBuf::from(root).join(candidate)
    };
    let canonical = absolute.canonicalize().map_err(|_| "The file path no longer exists.".to_owned())?;
    if let Some(root) = project_root {
        let canonical_root = PathBuf::from(root).canonicalize().map_err(|_| "The project folder no longer exists.".to_owned())?;
        if !canonical.starts_with(&canonical_root) {
            return Err("The file is outside this conversation's project folder.".to_owned());
        }
    }
    if !canonical.is_file() {
        return Err("That path is not a file.".to_owned());
    }
    Ok(canonical)
}

/// Highlight a file in File Explorer. **The only thing this window may do with a
/// file the owner did not pick themselves.**
///
/// A message names paths a bot chose, so opening one with its default program
/// would run whatever the bot wrote: a `.bat`, a `.ps1`, a `.lnk` or an `.exe` is
/// one click away from executing. The app never does it either: a file chip
/// previews the file or shows it in its folder, nothing more. `/select,` only
/// highlights the file; it runs nothing.
pub fn show_in_folder(path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy().to_string();
    // `explorer /select,<path>` takes two arguments on Windows, and the path must
    // be the canonical one, or Explorer shows Documents instead.
    Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not show the file in its folder: {error}"))
}

/// Open a link in the browser, after the same check the markdown makes. This
/// never goes through a file handler: only `http`/`https` survive
/// `safe_http_url`, and Windows' own "open" verb sends those to the browser.
pub fn open_link(url: &str) -> Result<(), String> {
    let url = crate::ui::markdown::safe_http_url(url).ok_or_else(|| "That link is not one Bitbots opens.".to_owned())?;
    shell_open(&url)
}

/// Hand one `http`/`https` URL to Windows' default handler.
#[cfg(windows)]
fn shell_open(url: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let verb = wide("open");
    let file = wide(url);
    // A result above 32 is success; anything else is an error code.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) <= 32 {
        return Err("Windows could not open that link.".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn shell_open(_url: &str) -> Result<(), String> {
    Err("Links are only opened on Windows.".to_owned())
}

/// The kinds of file a chip may show inside the window, as `MediaPreview` does:
/// pictures, and text it can read without a program of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    Image,
    Text,
}

/// The extensions a chip may offer a Preview for. The bytes decide in the end
/// (`read_preview`), because a name can lie.
pub fn previewable_name(name: &str) -> bool {
    let extension = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let images = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
    let text = [
        "txt", "md", "markdown", "json", "jsonl", "log", "csv", "tsv", "rs", "toml", "yaml", "yml", "ini", "cfg", "html",
        "htm", "css", "scss", "js", "jsx", "ts", "tsx", "sql", "py", "rb", "go", "java", "c", "h", "cpp", "hpp", "cs",
        "sh", "xml", "svg", "diff", "patch", "gitignore", "env",
    ];
    images.contains(&extension.as_str()) || text.contains(&extension.as_str())
}

/// How much of a file the viewer reads and shows.
pub const PREVIEW_READ_BYTES: u64 = 200 * 1024;
pub const PREVIEW_MAX_LINES: usize = 4_000;

/// The preview a file's name and bytes allow, or `None` when Bitbots will not
/// show it (a program, an archive, a PDF, a video).
pub fn preview_kind(name: &str, bytes: &[u8]) -> Option<PreviewKind> {
    if image_format(bytes).is_some() {
        return Some(PreviewKind::Image);
    }
    if !previewable_name(name) {
        return None;
    }
    let extension = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let text = [
        "txt", "md", "markdown", "json", "jsonl", "log", "csv", "tsv", "rs", "toml", "yaml", "yml", "ini", "cfg", "html",
        "htm", "css", "scss", "js", "jsx", "ts", "tsx", "sql", "py", "rb", "go", "java", "c", "h", "cpp", "hpp", "cs",
        "sh", "bat", "cmd", "ps1", "xml", "svg", "diff", "patch", "gitignore", "env",
    ];
    if !text.contains(&extension.as_str()) {
        return None;
    }
    // Text is text only when the part we read holds no NUL byte.
    if bytes.iter().take(8_192).any(|byte| *byte == 0) {
        return None;
    }
    Some(PreviewKind::Text)
}

/// Read a file for the in-app viewer. Only ever called for a path
/// `canonical_project_file` already accepted, so it is inside the project.
pub fn read_preview(path: &Path) -> Result<(PreviewKind, Vec<u8>), String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|_| "That file could not be read.".to_owned())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(PREVIEW_READ_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "That file could not be read.".to_owned())?;
    let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let kind = preview_kind(&name, &bytes).ok_or_else(|| "Bitbots cannot show this kind of file.".to_owned())?;
    bytes.truncate(PREVIEW_READ_BYTES as usize);
    Ok((kind, bytes))
}

/// The text a viewer shows: at most `PREVIEW_MAX_LINES` lines, with a marker
/// saying more follows. Tabs become four spaces so code lines up.
pub fn preview_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ");
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() > PREVIEW_MAX_LINES {
        let mut shown = lines[..PREVIEW_MAX_LINES].join("\n");
        shown.push_str("\n… the rest of this file is not shown.");
        shown
    } else {
        text
    }
}

/// Decode a `data:<type>;base64,<bytes>` URL — what `message.image` answers with.
pub fn decode_data_url(data_url: &str) -> Option<Vec<u8>> {
    let (meta, body) = data_url.split_once(",")?;
    if !meta.starts_with("data:") || !meta.ends_with(";base64") {
        return None;
    }
    decode_base64(body.trim())
}

/// Standard base64, which is what the daemon writes. Padding is optional, as a
/// provider's own URL may leave it out.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u8;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_images_kind_comes_from_its_own_bytes_not_its_name() {
        assert_eq!(attachment_extension(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]), Some("png"));
        assert_eq!(attachment_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(attachment_extension(b"GIF89a..."), Some("gif"));
        assert_eq!(attachment_extension(b"RIFF____WEBPVP8 "), Some("webp"));
        assert_eq!(attachment_extension(b"not an image"), None);
        assert_eq!(image_format(b"not an image"), None);
    }

    #[test]
    fn a_name_is_made_into_a_safe_stem() {
        assert_eq!(clean_stem("Screenshot 2026-11-08 101500.png"), "Screenshot-2026-11-08-101500-png");
        assert_eq!(clean_stem("../../../etc/passwd"), "passwd");
        assert_eq!(clean_stem(""), "attachment");
        assert_eq!(clean_stem("C:\\Users\\me\\shot.png"), "shot-png");
    }

    #[test]
    fn a_staged_image_is_written_and_says_where() {
        let folder = std::env::temp_dir().join(format!("bitbots-native-attach-{}", std::process::id()));
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
        let path = stage_attachment(&folder, &png, "pasted image.png").expect("staged");
        assert!(path.exists());
        assert!(path.extension().and_then(|ext| ext.to_str()) == Some("png"));
        assert_eq!(std::fs::read(&path).unwrap(), png.to_vec());
        assert!(stage_attachment(&folder, b"nothing", "x.txt").is_err());
        assert!(stage_attachment(&folder, &[], "x.png").is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_file_outside_the_project_is_refused() {
        let root = std::env::temp_dir().join(format!("bitbots-native-files-{}", std::process::id()));
        let inner = root.join("inside.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&inner, b"hello").unwrap();
        let root_text = root.to_string_lossy().to_string();
        let inside = canonical_project_file("inside.txt", Some(&root_text)).expect("a relative file in the project");
        assert!(inside.ends_with("inside.txt"));
        let outside = std::env::temp_dir().join("bitbots-native-outside.txt");
        std::fs::write(&outside, b"no").unwrap();
        assert!(canonical_project_file(&outside.to_string_lossy(), Some(&root_text)).is_err());
        assert!(canonical_project_file(&root_text, Some(&root_text)).is_err(), "a folder is not a file");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn a_link_is_opened_only_when_it_is_a_safe_url() {
        // The check itself lives in the markdown; this proves the gate is used.
        assert!(open_link("javascript:alert(1)").is_err());
        assert!(open_link("file:///C:/Windows").is_err());
    }

    #[test]
    fn a_data_url_becomes_the_png_bytes_it_holds() {
        let bytes = decode_data_url("data:image/png;base64,iVBORw0KGgo=").expect("decoded");
        assert_eq!(bytes, vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(decode_base64("aGVsbG8="), Some(b"hello".to_vec()));
        assert!(decode_data_url("data:image/png,notbase64").is_none());
        assert!(decode_base64("!!!").is_none(), "a broken body is not a picture");
    }
}

/// The window's one rule about other programs: nothing it does may run a file a
/// bot named. These tests read the source, because the rule is about every call
/// site, including the ones added after this line.
#[cfg(test)]
mod spawn_guard {
    use std::path::{Path, PathBuf};

    /// The only programs the window may start, and why.
    const ALLOWED: [(&str, &str); 3] = [
        ("files.rs", "explorer.exe /select, only: it highlights a file and runs nothing"),
        ("notify.rs", "reg add, to register the notification app id once"),
        ("supervisor.rs", "bitbotsd.exe, the daemon this window owns"),
    ];

    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("the source folder") {
            let entry = entry.expect("an entry");
            let path = entry.path();
            if path.is_dir() {
                sources(&path, found);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                found.push(path);
            }
        }
    }

    #[test]
    fn only_the_three_allowed_places_start_a_program() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&src, &mut files);
        assert!(files.len() > 10, "the walk found the source files");
        // Built by concatenation so this test's own text never matches it.
        let needle = format!("Command::{}", "new(");
        let mut sites: Vec<(String, usize)> = Vec::new();
        for file in &files {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let text = std::fs::read_to_string(file).unwrap_or_default();
            for (index, line) in text.lines().enumerate() {
                if line.contains(&needle) {
                    sites.push((name.clone(), index + 1));
                }
            }
        }
        assert!(!sites.is_empty(), "the scan sees the real call sites");
        for (file, line) in &sites {
            assert!(
                ALLOWED.iter().any(|(allowed, _)| allowed == file),
                "{file}:{line} starts a program, and only files.rs (explorer /select,), notify.rs (reg) and supervisor.rs (bitbotsd) may"
            );
        }
    }

    #[test]
    fn the_only_program_files_rs_starts_is_explorer_highlighting_a_file() {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/files.rs")).unwrap();
        let needle = format!("Command::{}", "new(");
        let spawn_lines: Vec<&str> = text.lines().filter(|line| line.contains(&needle)).collect();
        assert_eq!(spawn_lines.len(), 1, "one spawn in this file: {spawn_lines:?}");
        assert!(spawn_lines[0].contains("explorer.exe"), "{}", spawn_lines[0]);
        // And it is `/select,`, never a bare path.
        let show = text.find("pub fn show_in_folder").expect("the one function that spawns");
        let body = &text[show..show + 700];
        assert!(body.contains("\"/select,\""), "the file is only highlighted: {body}");
        // The function that opened a file with its own program is gone, by name.
        let banned = format!("open_with_default{}", "_app");
        assert!(!text.contains(&banned), "{banned} must not exist");
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn only_pictures_and_text_can_be_shown_in_the_window() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        assert_eq!(preview_kind("shot.png", &png), Some(PreviewKind::Image));
        assert_eq!(preview_kind("notes.md", b"# Hello"), Some(PreviewKind::Text));
        assert_eq!(preview_kind("data.json", b"{}"), Some(PreviewKind::Text));
        // A program is never previewed, whatever it is called.
        assert_eq!(preview_kind("run.bat", b"@echo off"), None);
        assert_eq!(preview_kind("setup.exe", b"MZ"), None);
        assert_eq!(preview_kind("tool.ps1", b"Write-Host hi"), None);
        assert_eq!(preview_kind("shortcut.lnk", b"L"), None);
        assert_eq!(preview_kind("report.pdf", b"%PDF-1.7"), None);
        // A binary that lies about its name is not text either.
        assert_eq!(preview_kind("fake.txt", &[0x00, 0x01, 0x02]), None);
    }

    #[test]
    fn the_chip_offers_a_preview_only_for_names_it_can_show() {
        for name in ["shot.png", "photo.JPG", "notes.md", "src/main.rs", "data.json", "a.diff"] {
            assert!(previewable_name(name), "{name}");
        }
        for name in ["run.bat", "setup.exe", "tool.ps1", "shortcut.lnk", "report.pdf", "movie.mp4", "archive.zip"] {
            assert!(!previewable_name(name), "{name}");
        }
    }

    #[test]
    fn text_is_shown_with_its_tabs_expanded_and_a_marker_when_it_is_long() {
        assert_eq!(preview_text(b"a\tb\r\nc"), "a    b\nc");
        let many = "line\n".repeat(PREVIEW_MAX_LINES + 5);
        let shown = preview_text(many.as_bytes());
        assert!(shown.ends_with("… the rest of this file is not shown."));
        assert_eq!(shown.lines().count(), PREVIEW_MAX_LINES + 1);
    }
}
