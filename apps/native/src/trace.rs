//! A plain append-only log beside the app's own data, for the live check and for
//! the owner to send back. It never records a capability, a token or message
//! text: only what happened, in words.

use std::io::Write;

use crate::paths;

pub fn log(message: impl AsRef<str>) {
    let line = format!(
        "{} {}\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        message.as_ref()
    );
    println!("{}", line.trim_end());
    append(&line);
}

/// Append one line to `native.log`, ignoring any failure: the logger must never
/// be the thing that kills the window.
fn append(line: &str) {
    if let Ok(folder) = paths::ensure_native_data_dir() {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(folder.join("native.log")) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

/// Catch a panic and write it to the window's own log, with its location and a
/// backtrace.
///
/// A panic in the window kills the process without a word: the daemon child is
/// dropped first (so the log ends at "daemon child stopped"), Rust prints the
/// message to a stderr nobody is watching, and `main` never reaches "exited".
/// The hook is installed first thing in `main`, so any later panic leaves a
/// `PANIC at <file>:<line>:<column>` line behind.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_owned()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "a panic with no message".to_owned()
        };
        let location = info
            .location()
            .map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column()))
            .unwrap_or_else(|| "an unknown place".to_owned());
        let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        // Written straight to the file: `log` also prints, and a closed stdout
        // must not turn one panic into two.
        let backtrace = std::backtrace::Backtrace::force_capture();
        append(&format!("{stamp} PANIC at {location}: {message}\n"));
        append(&format!("{stamp} PANIC backtrace:\n{backtrace}"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logger_never_fails_on_a_bad_line() {
        // A newline in the message must not split the file's own format.
        log("a line with two\nlines");
        log("a line with a \u{0} nul");
    }
}
