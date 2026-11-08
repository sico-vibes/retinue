//! The words dictation shows, ported from `ui/dictation.ts` and the React app's
//! `DictationIsland` copy.
//!
//! The island itself never says what was heard — the transcript goes to the
//! message box — so every message here is status or an explanation of what went
//! wrong and what to do next. `friendlyDictationError`'s tests come with it.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

/// `140 MB`, `1.2 GB`: the size shown before a speech model is downloaded.
pub fn format_model_size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes.filter(|bytes| *bytes > 0) else { return String::new() };
    let megabytes = bytes as f64 / (1024.0 * 1024.0);
    if megabytes >= 1024.0 {
        format!("{:.1} GB", megabytes / 1024.0)
    } else {
        format!("{} MB", megabytes.round() as u64)
    }
}

/// The React app's download prompt, word for word, as the one message this slice
/// can honestly show: there is no download flow yet.
///
/// `SpeechModelDialog.tsx`: "Dictation on this device needs the {label} model
/// {size}. It downloads once, then runs offline. Your audio stays on this
/// computer."
pub fn no_model_message(label: &str, size_bytes: Option<u64>) -> String {
    let size = format_model_size(size_bytes);
    let sized = if size.is_empty() { String::new() } else { format!(", {size}") };
    format!(
        "Dictation on this device needs the {label} model{sized}. It downloads once, then runs offline. Your audio stays on this computer."
    )
}

/// The machine reasons the native layer gives, in plain words
/// (`engineReasonText`).
pub fn engine_reason_text(reason: &str) -> &'static str {
    match reason {
        "auth_missing" => "Codex is not signed in",
        "auth_required" => "the Codex sign-in needs refreshing",
        "rate_limited" => "it is rate limited right now",
        "challenged" => "the service asked for a browser check",
        "unavailable" => "the service could not be reached",
        "too_large" | "too_long" => "the recording was too long",
        "too_short" => "the recording was too short",
        "bad_response" | "rejected" => "the service refused the recording",
        "consent_required" => "permission has not been given",
        "model_not_ready" | "speech_model_not_ready" => "no on-device model is installed",
        _ => "",
    }
}

/// Whether a message is the native layer saying the model is not installed
/// (`mentionsMissingLocalModel`).
pub fn mentions_missing_model(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("no on-device model is installed") || lower.contains("on-device speech model is not installed") || lower.contains("not installed yet")
}

/// Restate a terse native failure in plain language, with what to do next.
pub fn friendly_error(raw: &str) -> String {
    let text = raw.trim();
    if text.starts_with("OpenAI blocks transcription from apps that are not its own clients") {
        return "OpenAI's bot check blocks ChatGPT dictation from third-party apps, and the on-device speech model is not installed yet. Download it in Settings, Speech.".to_owned();
    }
    if let Some(reason) = inside_parens_after(text, "ChatGPT transcription is unavailable (") {
        return format!(
            "ChatGPT dictation did not work: {}. Download the on-device speech model in Settings, Speech, to dictate offline.",
            plain_reason(reason)
        );
    }
    if let Some(reason) = inside_parens_after(text, "ChatGPT transcription failed (") {
        return format!(
            "ChatGPT dictation did not work: {}. The on-device model could not start either; check Settings, Speech.",
            plain_reason(reason)
        );
    }
    text.to_owned()
}

/// The parenthesised reason after `prefix`, if the message has that shape.
fn inside_parens_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(prefix)?;
    let end = rest.find(')')?;
    Some(rest[..end].trim())
}

/// `CLOUD_REASON_COPY`: the machine reason in plain words.
fn plain_reason(reason: &str) -> String {
    match reason.to_lowercase().as_str() {
        "verification required" => "OpenAI's bot check blocked it".to_owned(),
        "request rejected" | "rejected" => "ChatGPT refused the recording".to_owned(),
        "sign-in required" => "you are not signed in to Codex".to_owned(),
        "sign-in expired" => "your Codex sign-in expired (run `codex login` in a terminal)".to_owned(),
        "rate limited" => "ChatGPT is rate limiting you right now".to_owned(),
        "unavailable" => "ChatGPT could not be reached".to_owned(),
        "recording too large" => "the recording was too long".to_owned(),
        "recording too short" => "the recording was too short".to_owned(),
        "unreadable response" => "ChatGPT sent back something unreadable".to_owned(),
        other => other.to_owned(),
    }
}

/// The island's status line for a phase (`DictationIsland`'s `label`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IslandPhase {
    Listening,
    Processing,
    Done,
    Silent,
    Error,
}

impl IslandPhase {
    pub fn key(self) -> &'static str {
        match self {
            IslandPhase::Listening => "listening",
            IslandPhase::Processing => "processing",
            IslandPhase::Done => "done",
            IslandPhase::Silent => "silent",
            IslandPhase::Error => "error",
        }
    }

    /// What a screen reader hears (`aria-label`).
    pub fn label(self, warming: bool) -> &'static str {
        match self {
            IslandPhase::Processing => "Transcribing",
            IslandPhase::Done => "Dictation added",
            IslandPhase::Silent => "Nothing was heard",
            IslandPhase::Error => "Dictation stopped",
            IslandPhase::Listening => {
                if warming {
                    "Listening, the speech model is loading"
                } else {
                    "Listening"
                }
            }
        }
    }

    /// The note beside the timer, when there is one.
    pub fn note(self, warming: bool) -> Option<&'static str> {
        match self {
            IslandPhase::Listening if warming => Some("Getting ready"),
            IslandPhase::Done => Some("Added to your message"),
            IslandPhase::Silent => Some("Nothing heard"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bot_check_wording_is_explained_and_points_at_settings() {
        let text = friendly_error("ChatGPT transcription is unavailable (verification required) and no on-device model is installed.");
        assert!(text.contains("OpenAI's bot check blocked it"), "{text}");
        assert!(text.contains("Settings, Speech"), "{text}");
        assert!(!text.contains("verification required"), "{text}");
    }

    #[test]
    fn the_fallback_failure_and_the_challenged_wording_map_too() {
        assert!(friendly_error("OpenAI blocks transcription from apps that are not its own clients, and no on-device model is installed. Use the on-device model or dictate inside the ChatGPT app.").contains("third-party apps"));
        assert!(friendly_error("ChatGPT transcription failed (rate limited), and the on-device model could not run.").contains("rate limiting"));
    }

    #[test]
    fn any_other_message_is_left_alone() {
        assert_eq!(friendly_error("The microphone is busy."), "The microphone is busy.");
        assert_eq!(friendly_error("  spaced  "), "spaced");
    }

    #[test]
    fn the_no_model_message_is_the_react_apps_download_prompt() {
        // Parakeet TDT v3's four files, as the catalog adds them up.
        let bytes = 652_184_281 + 11_845_275 + 6_355_277 + 93_939;
        assert_eq!(
            no_model_message("Parakeet TDT v3", Some(bytes)),
            "Dictation on this device needs the Parakeet TDT v3 model, 639 MB. It downloads once, then runs offline. Your audio stays on this computer."
        );
        // A model whose size is unknown still names itself.
        assert_eq!(
            no_model_message("Speech model", None),
            "Dictation on this device needs the Speech model model. It downloads once, then runs offline. Your audio stays on this computer."
        );
        assert_eq!(format_model_size(None), "");
        assert_eq!(format_model_size(Some(0)), "");
        assert_eq!(format_model_size(Some(140 * 1024 * 1024)), "140 MB");
        assert_eq!(format_model_size(Some(1258 * 1024 * 1024)), "1.2 GB");
    }

    #[test]
    fn the_island_labels_are_the_react_apps() {
        assert_eq!(IslandPhase::Processing.label(false), "Transcribing");
        assert_eq!(IslandPhase::Done.label(false), "Dictation added");
        assert_eq!(IslandPhase::Silent.label(false), "Nothing was heard");
        assert_eq!(IslandPhase::Error.label(false), "Dictation stopped");
        assert_eq!(IslandPhase::Listening.label(true), "Listening, the speech model is loading");
        assert_eq!(IslandPhase::Listening.label(false), "Listening");
        assert_eq!(IslandPhase::Listening.note(true), Some("Getting ready"));
        assert_eq!(IslandPhase::Done.note(false), Some("Added to your message"));
        assert_eq!(IslandPhase::Silent.note(false), Some("Nothing heard"));
        assert_eq!(IslandPhase::Processing.note(false), None);
        assert!(mentions_missing_model("The on-device speech model is not installed yet."));
        assert!(!mentions_missing_model("The microphone is busy."));
        assert_eq!(engine_reason_text("model_not_ready"), "no on-device model is installed");
    }
}
