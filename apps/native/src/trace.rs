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
    if let Ok(folder) = paths::ensure_native_data_dir() {
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(folder.join("native.log")) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}
