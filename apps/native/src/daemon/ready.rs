//! The one JSON object the daemon writes to stdout before any log line.
//!
//! The capability must never be logged, printed or persisted, so `ReadyLine` has
//! a redacting `Debug` implementation and only the client builds an auth header
//! from it.

use std::fmt;

use serde::Deserialize;

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReadyLine {
    pub protocol_version: u32,
    pub address: String,
    pub capability: String,
}

impl fmt::Debug for ReadyLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadyLine")
            .field("protocol_version", &self.protocol_version)
            .field("address", &self.address)
            .field("capability", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyError {
    InvalidLine,
    WrongProtocol(u32),
    NotLoopback,
    EmptyCapability,
}

impl fmt::Display for ReadyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadyError::InvalidLine => {
                write!(formatter, "Daemon returned an invalid bootstrap line.")
            }
            ReadyError::WrongProtocol(version) => write!(
                formatter,
                "Daemon speaks protocol {version}, but this window only speaks protocol 1."
            ),
            ReadyError::NotLoopback => write!(
                formatter,
                "Daemon did not bind 127.0.0.1; the window will not talk to it."
            ),
            ReadyError::EmptyCapability => {
                write!(formatter, "Daemon bootstrap carried no capability.")
            }
        }
    }
}

/// The same check the Tauri shell makes: protocol 1, a loopback address and a
/// capability. Anything else is refused before a single request is sent.
pub fn parse_ready(line: &str) -> Result<ReadyLine, ReadyError> {
    let ready: ReadyLine =
        serde_json::from_str(line.trim()).map_err(|_| ReadyError::InvalidLine)?;
    if ready.protocol_version != 1 {
        return Err(ReadyError::WrongProtocol(ready.protocol_version));
    }
    if !ready.address.starts_with("127.0.0.1:") {
        return Err(ReadyError::NotLoopback);
    }
    if ready.capability.is_empty() {
        return Err(ReadyError::EmptyCapability);
    }
    Ok(ready)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"type":"bloblexd.ready","protocolVersion":1,"address":"127.0.0.1:49152","capability":"secret-value"}"#;

    #[test]
    fn a_valid_bootstrap_line_is_accepted() {
        let ready = parse_ready(VALID).expect("valid line");
        assert_eq!(ready.address, "127.0.0.1:49152");
        assert_eq!(ready.protocol_version, 1);
        assert!(!format!("{ready:?}").contains("secret-value"), "Debug must not print the capability");
    }

    #[test]
    fn another_protocol_version_is_refused() {
        let line = VALID.replace("\"protocolVersion\":1", "\"protocolVersion\":2");
        assert_eq!(parse_ready(&line), Err(ReadyError::WrongProtocol(2)));
    }

    #[test]
    fn a_non_loopback_address_is_refused() {
        for address in ["0.0.0.0:49152", "192.168.1.5:49152", "[::1]:49152"] {
            let line = VALID.replace("127.0.0.1:49152", address);
            assert_eq!(parse_ready(&line), Err(ReadyError::NotLoopback), "{address}");
        }
    }

    #[test]
    fn an_empty_capability_is_refused() {
        let line = VALID.replace("\"secret-value\"", "\"\"");
        assert_eq!(parse_ready(&line), Err(ReadyError::EmptyCapability));
    }

    #[test]
    fn a_line_that_is_not_json_is_refused() {
        assert_eq!(parse_ready("starting bloblexd…"), Err(ReadyError::InvalidLine));
        assert_eq!(parse_ready(""), Err(ReadyError::InvalidLine));
    }
}
