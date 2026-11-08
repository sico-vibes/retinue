//! The Windows notification for a turn that finished or needs the owner while
//! the window is in the background (`ui/notificationTransitions.ts`, sent by
//! `desktopIntegrations.sendDesktopNotification`).
//!
//! The app's wording is kept exactly: "Finished: <title>", "Failed: <title>" and
//! "Needs your approval: <title>", with the bot's name above it.
//!
//! An unpackaged Win32 app may only raise a toast under an app id Windows knows,
//! so the first notification registers `Bitbots.Native` under the owner's own
//! `AppUserModelId` key. A refusal here is never fatal: the caller falls back to
//! its own line in the window.

use std::sync::Once;

/// The app id the window's notifications arrive under.
pub const APP_ID: &str = "Bitbots.Native";

/// What a notification says, from the transition that caused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Finished,
    Failed,
    Approval,
}

impl Notice {
    /// The body, as `notificationForTransition` words it.
    pub fn body(self, title: &str) -> String {
        let title = title.trim();
        let title = if title.is_empty() { "Untitled conversation" } else { title };
        match self {
            Notice::Finished => format!("Finished: {title}"),
            Notice::Failed => format!("Failed: {title}"),
            Notice::Approval => format!("Needs your approval: {title}"),
        }
    }
}

/// The transition worth a notification, or `None` when nothing changed.
/// `previous` is absent for a conversation this window has not seen before, and
/// then nothing is announced: the first sight of a busy bot is not news.
pub fn notice_for(previous: Option<&str>, next: &str) -> Option<Notice> {
    let previous = previous?;
    let prior = previous.to_lowercase();
    let state = next.to_lowercase();
    if state == "completed" && prior != "completed" {
        return Some(Notice::Finished);
    }
    if (state == "error" || state == "failed") && prior != state {
        return Some(Notice::Failed);
    }
    if state == "waiting_permission" && prior != "waiting_permission" {
        return Some(Notice::Approval);
    }
    None
}

/// A bot's notification level: `all`, `input` (approvals only) or `muted`.
pub fn allowed(level: Option<&str>, notice: Notice) -> bool {
    match level.unwrap_or("").trim().to_lowercase().as_str() {
        "muted" => false,
        "input" => notice == Notice::Approval,
        _ => true,
    }
}

static REGISTER: Once = Once::new();

/// Make Windows accept a toast from this app. One `reg` call, once per run, and
/// a refusal is ignored: the worst case is that no toast appears.
fn register_app_id() {
    REGISTER.call_once(|| {
        let _ = std::process::Command::new("reg")
            .args([
                "add",
                &format!("HKCU\\Software\\Classes\\AppUserModelId\\{APP_ID}"),
                "/v",
                "DisplayName",
                "/t",
                "REG_SZ",
                "/d",
                "Bitbots",
                "/f",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Escape the three characters an XML text node cannot hold.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // A toast line cannot hold a newline either; it breaks it up.
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

/// The toast's XML: the bot's name, then the sentence.
pub fn toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_escape(title),
        xml_escape(body)
    )
}

/// Raise one toast. Returns the reason when Windows refused it, so the caller
/// can say something in the window instead.
#[cfg(windows)]
pub fn notify(title: &str, body: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    register_app_id();
    let xml = toast_xml(title, body);
    let document = XmlDocument::new().map_err(|error| format!("A notification could not be prepared: {error}"))?;
    document
        .LoadXml(&HSTRING::from(xml))
        .map_err(|error| format!("A notification could not be prepared: {error}"))?;
    let toast = ToastNotification::CreateToastNotification(&document)
        .map_err(|error| format!("A notification could not be created: {error}"))?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))
        .map_err(|error| format!("Notifications are not set up on this computer: {error}"))?;
    notifier
        .Show(&toast)
        .map_err(|error| format!("The notification could not be shown: {error}"))
}

#[cfg(not(windows))]
pub fn notify(_title: &str, _body: &str) -> Result<(), String> {
    Err("Notifications are only shown on Windows.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_turn_is_announced_once_per_state() {
        assert_eq!(notice_for(Some("working"), "completed"), Some(Notice::Finished));
        assert_eq!(notice_for(Some("completed"), "completed"), None, "no repeat while it stays finished");
        assert_eq!(notice_for(Some("idle"), "working"), None, "starting work is not news");
        assert_eq!(notice_for(None, "completed"), None, "a new conversation is not news");
    }

    #[test]
    fn a_failure_and_a_waiting_approval_are_announced() {
        assert_eq!(notice_for(Some("working"), "error"), Some(Notice::Failed));
        assert_eq!(notice_for(Some("error"), "failed"), Some(Notice::Failed));
        assert_eq!(notice_for(Some("working"), "waiting_permission"), Some(Notice::Approval));
        assert_eq!(notice_for(Some("waiting_permission"), "waiting_permission"), None);
    }

    #[test]
    fn the_words_are_the_apps_own() {
        assert_eq!(Notice::Finished.body("Fix the tests"), "Finished: Fix the tests");
        assert_eq!(Notice::Failed.body("Fix the tests"), "Failed: Fix the tests");
        assert_eq!(Notice::Approval.body("Fix the tests"), "Needs your approval: Fix the tests");
        assert_eq!(Notice::Finished.body("   "), "Finished: Untitled conversation");
    }

    #[test]
    fn a_bots_level_decides_which_notifications_it_may_send() {
        assert!(allowed(None, Notice::Finished));
        assert!(allowed(Some("all"), Notice::Failed));
        assert!(!allowed(Some("input"), Notice::Finished));
        assert!(allowed(Some("input"), Notice::Approval));
        assert!(!allowed(Some("muted"), Notice::Approval));
    }

    #[test]
    fn the_toast_markup_cannot_be_broken_by_a_title() {
        let xml = toast_xml("Ada <&>", "Finished: a & b");
        assert!(xml.contains("Ada &lt;&amp;&gt;"), "{xml}");
        assert!(xml.contains("a &amp; b"));
        assert!(!xml.contains("<&>"));
    }
}
