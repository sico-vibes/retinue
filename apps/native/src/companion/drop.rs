//! The companion's drop zone: a file dragged onto the island is sent to the
//! picked conversation, or starts one (`apps/desktop/src/ui/companionDrop.ts`).
//!
//! The phase machine is pure, as it is in the React app: the view owns the
//! grace timer and the card.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

/// How long after a drag leaves the island before it collapses.
pub const DROP_LEAVE_GRACE_MS: u64 = 250;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPhase {
    Idle,
    /// A drag is on the island.
    Over,
    /// A drag just left; it may come back within the grace period.
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropEvent {
    Enter,
    Leave,
    Drop,
}

/// `advanceDropZone`: a drop always ends at idle, entering always goes to over,
/// and leaving only steps back while the drag is on the island.
pub fn advance_drop_zone(phase: DropPhase, event: DropEvent) -> DropPhase {
    match event {
        DropEvent::Drop => DropPhase::Idle,
        DropEvent::Enter => DropPhase::Over,
        DropEvent::Leave => {
            if phase == DropPhase::Over {
                DropPhase::Pending
            } else {
                phase
            }
        }
    }
}

/// Whether the island shows the drop card (a drag is on it, or may return).
pub fn drop_zone_active(phase: DropPhase) -> bool {
    phase != DropPhase::Idle
}

/// Whether a pending drag's grace period has run out.
pub fn grace_over(left_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(left_at_ms) >= DROP_LEAVE_GRACE_MS
}

/// The file-name chip: name, upper-case extension, readable size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropChips {
    pub name: String,
    pub kind: String,
    pub size: String,
}

pub fn drop_chips(path: &str, size_bytes: Option<u64>) -> DropChips {
    let name = path.rsplit(['\\', '/']).next().filter(|part| !part.is_empty()).unwrap_or(path).to_owned();
    let kind = extension_of(&name).map(|value| value.to_uppercase()).unwrap_or_else(|| "File".to_owned());
    let size = size_bytes.map(format_drop_size).unwrap_or_else(|| "—".to_owned());
    DropChips { name, kind, size }
}

fn extension_of(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.is_empty() || extension.is_empty() || extension.len() > 12 {
        return None;
    }
    extension.chars().all(|ch| ch.is_ascii_alphanumeric()).then(|| extension.to_owned())
}

fn format_drop_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// What a drop asks the daemon to do: a prompt that names the file, to the
/// picked conversation, or a new conversation when none is picked.
///
/// The React app stages the path as a reference chip ("path only") inside its
/// own draft and sends it with the next message; the native preview has no draft
/// chips yet, so the drop sends one short prompt that names the path.
pub fn drop_prompt(path: &str) -> String {
    let chips = drop_chips(path, None);
    format!("Take a look at this file: {}", chips.name).trim_end().to_owned() + "\n\n" + path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zone_opens_once_and_stays_over_while_the_drag_is_on_the_island() {
        assert_eq!(advance_drop_zone(DropPhase::Idle, DropEvent::Enter), DropPhase::Over);
        assert_eq!(advance_drop_zone(DropPhase::Over, DropEvent::Enter), DropPhase::Over);
        assert_eq!(advance_drop_zone(DropPhase::Over, DropEvent::Leave), DropPhase::Pending);
    }

    #[test]
    fn coming_back_within_the_grace_period_never_idles() {
        assert_eq!(advance_drop_zone(DropPhase::Pending, DropEvent::Enter), DropPhase::Over);
        let mut phase = advance_drop_zone(DropPhase::Idle, DropEvent::Enter);
        phase = advance_drop_zone(phase, DropEvent::Leave);
        phase = advance_drop_zone(phase, DropEvent::Enter);
        phase = advance_drop_zone(phase, DropEvent::Leave);
        assert_eq!(phase, DropPhase::Pending);
        assert!(!grace_over(1_000, 1_200));
        assert!(grace_over(1_000, 1_250));
    }

    #[test]
    fn a_drop_always_ends_at_idle_and_a_stray_leave_changes_nothing() {
        assert_eq!(advance_drop_zone(DropPhase::Over, DropEvent::Drop), DropPhase::Idle);
        assert_eq!(advance_drop_zone(DropPhase::Pending, DropEvent::Drop), DropPhase::Idle);
        assert_eq!(advance_drop_zone(DropPhase::Idle, DropEvent::Leave), DropPhase::Idle);
        assert!(!drop_zone_active(DropPhase::Idle));
        assert!(drop_zone_active(DropPhase::Over) && drop_zone_active(DropPhase::Pending));
    }

    #[test]
    fn the_chips_split_a_windows_path_into_name_type_and_size() {
        assert_eq!(
            drop_chips("C:\\work\\site\\report.PDF", Some(2048)),
            DropChips { name: "report.PDF".into(), kind: "PDF".into(), size: "2.0 KB".into() }
        );
        assert_eq!(
            drop_chips("/home/me/notes.md", Some(512)),
            DropChips { name: "notes.md".into(), kind: "MD".into(), size: "512 B".into() }
        );
        assert_eq!(
            drop_chips("C:\\work\\big.zip", Some(5 * 1024 * 1024)),
            DropChips { name: "big.zip".into(), kind: "ZIP".into(), size: "5.0 MB".into() }
        );
    }

    #[test]
    fn a_missing_extension_or_size_falls_back() {
        assert_eq!(drop_chips("C:\\work\\LICENSE", None), DropChips { name: "LICENSE".into(), kind: "File".into(), size: "—".into() });
        assert_eq!(drop_chips("C:\\work\\.hidden", Some(1)).kind, "File");
        assert_eq!(drop_chips("C:\\work\\a.verylongextension", Some(1)).kind, "File");
    }

    #[test]
    fn a_drop_sends_a_prompt_that_names_the_file_and_its_path() {
        let prompt = drop_prompt("C:\\work\\site\\report.PDF");
        assert!(prompt.starts_with("Take a look at this file: report.PDF"));
        assert!(prompt.contains("C:\\work\\site\\report.PDF"));
    }
}
