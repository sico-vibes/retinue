//! What needs the owner, and how much of the plan is left: the plan-usage line at
//! the foot of the sidebar (`ui/SidebarUsage.tsx` and `ui/QuotaBar.tsx`), the
//! bell that lists every waiting approval, question and proposal
//! (`ui/NeedsYou.tsx`), and the Windows notification a finished or waiting turn
//! raises while the window is in the background.
//!
//! Every number here comes from the daemon's own quota snapshots: a window the
//! provider did not report is absent, never zero.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::art::{self, Glyph};
use crate::state::quota::{self, QuotaProvider, QuotaTone};
use crate::state::Permission;
use crate::theme::{self, Colors};
use crate::ui::root::Root;

/// One thing waiting on the owner, as the bell lists it.
#[derive(Clone)]
pub struct NeedsItem {
    pub session_id: String,
    pub bot_name: String,
    pub kind: &'static str,
    pub summary: String,
    pub since: Option<i64>,
}

/// Everything waiting, oldest first. A request whose conversation is gone or
/// archived is not listed (`needsYouItems`).
pub fn needs_items(permissions: &[Permission], sessions: &[crate::state::Session], bots: &[crate::state::Agent]) -> Vec<NeedsItem> {
    let mut items: Vec<(NeedsItem, i64)> = Vec::new();
    for permission in permissions {
        let Some(session_id) = permission.session_id.clone() else { continue };
        let Some(session) = sessions.iter().find(|session| session.id == session_id) else { continue };
        if session.archived {
            continue;
        }
        let kind = if permission.is_question() {
            "Question"
        } else if permission.is_proposal() {
            "Proposal"
        } else {
            "Approval"
        };
        let raw = if permission.is_question() {
            permission
                .questions
                .first()
                .map(|question| question.question.clone().max_or(question.header.clone()))
                .or_else(|| permission.title.clone())
                .unwrap_or_default()
        } else {
            permission
                .title
                .clone()
                .or(permission.command.clone())
                .or(permission.tool.clone())
                .unwrap_or_default()
        };
        let summary = clip(&raw.split_whitespace().collect::<Vec<_>>().join(" "), 90);
        let bot_name = session
            .agent_id
            .as_deref()
            .and_then(|id| bots.iter().find(|agent| agent.id == id))
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "A bot".to_owned());
        let since = permission
            .created_at
            .as_deref()
            .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
            .map(|time| time.timestamp_millis());
        items.push((NeedsItem { session_id, bot_name, kind, summary, since }, since.unwrap_or(i64::MAX)));
    }
    items.sort_by_key(|(_, stamp)| *stamp);
    items.into_iter().map(|(item, _)| item).collect()
}

/// A small extension so a question's text wins over its header, as the app has
/// it: an empty question falls back to the header.
trait MaxOr {
    fn max_or(self, other: String) -> String;
}

impl MaxOr for String {
    fn max_or(self, other: String) -> String {
        if self.trim().is_empty() {
            other
        } else {
            self
        }
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        format!("{}…", text.chars().take(max.saturating_sub(1)).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// "just now", "12 min", "3 h": how long a request has waited.
pub fn waited(since: Option<i64>, now_ms: i64) -> Option<String> {
    let since = since?;
    let seconds = ((now_ms - since) / 1000).max(0);
    if seconds < 60 {
        return Some("just now".to_owned());
    }
    let minutes = (seconds + 30) / 60;
    if minutes < 60 {
        return Some(format!("{minutes} min"));
    }
    Some(format!("{} h", (minutes + 30) / 60))
}

impl Root {
    /// The plan-usage line: the limit closest to running out, across the coding
    /// agents. It renders nothing at all when there is no honest reading.
    pub(crate) fn render_usage_line(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.usage_bar {
            return None;
        }
        let rows = self.quota_rows();
        let current: Vec<QuotaProvider> = rows
            .iter()
            .filter(|row| !row.stale && row.status.is_none() && !row.windows.is_empty())
            .cloned()
            .collect();
        let (row, window) = quota::most_pressing(&current)?;
        let label = window.label.clone();
        let short = window.short.clone();
        let used = window.used_percent;
        let tone = quota::quota_tone(used);
        Some(
            div()
                .id("sidebar-usage")
                .mx(px(8.0))
                .mb(px(4.0))
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .gap(px(7.0))
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .child(art::glyph(if row.provider == "codex" { Glyph::ALERT } else { Glyph::CLOCK }, 12.0, colors.text_3))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(label.clone())),
                )
                .child(quota_meter(used, None, tone, colors))
                .child(
                    div()
                        .text_size(px(theme::FS_2XS))
                        .text_color(meter_tone(used, tone, colors))
                        .child(SharedString::from(format!("{used}%"))),
                )
                .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(short)))
                .child(div().flex_1())
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                    root.usage_open = !root.usage_open;
                    root.needs_open = false;
                    if root.usage_open {
                        root.load_quotas(cx);
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// The bell: a count of everything waiting, and the list behind it.
    pub(crate) fn render_needs_bell(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let items = self.needs_items();
        let count = items.len();
        let label = match count {
            0 => "Nothing needs you".to_owned(),
            1 => "1 request is waiting for you".to_owned(),
            other => format!("{other} requests are waiting for you"),
        };
        div()
            .id("needs-bell")
            .size(px(28.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .when(self.needs_open, |this| this.bg(colors.selected))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .child(art::glyph(Glyph::BELL, 14.0, if count > 0 { colors.status_working } else { colors.text_2 }))
            .when(count > 0, |this| {
                this.child(
                    div()
                        .ml(px(1.0))
                        .text_size(px(theme::FS_2XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.status_working)
                        .child(SharedString::from(if count > 9 { "9+".to_owned() } else { count.to_string() })),
                )
            })
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                root.needs_open = !root.needs_open;
                root.usage_open = false;
                let _ = &label;
                cx.notify();
            }))
            .into_any_element()
    }

    /// The bell's list, over the window's bottom-left corner.
    pub(crate) fn render_needs_panel(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.needs_open {
            return None;
        }
        let items = self.needs_items();
        let now = quota::now_ms();
        let mut panel = div()
            .id("needs-panel")
            .w(px(330.0))
            .max_h(px(420.0))
            .overflow_y_scroll()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .px(px(6.0))
                    .pb(px(4.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(if items.is_empty() {
                        "Nothing needs you.".to_owned()
                    } else {
                        "Waiting for you".to_owned()
                    })),
            );
        for item in items {
            let session_id = item.session_id.clone();
            let age = waited(item.since, now);
            let mut row = div()
                .id(SharedString::from(format!("needs-{}", item.session_id)))
                .px(px(6.0))
                .py(px(5.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover));
            row = row
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(theme::FS_XS))
                                .child(SharedString::from(if item.summary.is_empty() { "Open the request".to_owned() } else { item.summary.clone() })),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from(format!("{} · {}", item.bot_name, item.kind))),
                        ),
                )
                .when_some(age, |this, age| {
                    this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(age)))
                })
                .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                    root.needs_open = false;
                    if let Some(session) = root.snapshot().and_then(|snapshot| snapshot.session(&session_id)).cloned() {
                        if let Some(agent_id) = session.agent_id.clone() {
                            root.open_conversation(agent_id, session.id.clone(), cx);
                        }
                    }
                    cx.notify();
                }));
            panel = panel.child(row);
        }
        Some(
            div()
                .absolute()
                .left(px(8.0))
                .bottom(px(64.0))
                .child(panel)
                .into_any_element(),
        )
    }

    /// The usage panel behind the sidebar's line: every provider, every window.
    pub(crate) fn render_usage_panel(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.usage_open {
            return None;
        }
        let rows = self.quota_rows();
        let now = quota::now_ms();
        let newest = rows.iter().filter_map(|row| row.fetched_at.clone()).max();
        let updated = quota::updated_ago(newest.as_deref(), now).unwrap_or_else(|| "No reading yet".to_owned());
        let mut panel = div()
            .id("usage-panel")
            .w(px(360.0))
            .max_h(px(420.0))
            .overflow_y_scroll()
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong);
        if rows.is_empty() {
            panel = panel.child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_3)
                    .child("Signed-in Codex, Claude Code and OpenCode Go accounts report their plan limits here."),
            );
        }
        for row in rows {
            let mut section = div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().text_size(px(theme::FS_XS)).font_weight(FontWeight::SEMIBOLD).child(SharedString::from(row.name.clone())))
                        .when_some(row.status.clone(), |this, status| {
                            this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(status)))
                        })
                        .when(row.stale, |this| {
                            this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("Showing the last reading"))
                        }),
                );
            for window in &row.windows {
                let reset = if row.stale { None } else { quota::reset_in(window.resets_at.as_deref(), now) };
                let pace = if row.stale { None } else { quota::elapsed_percent(window, now) };
                let tone = quota::quota_tone(window.used_percent);
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .w(px(96.0))
                                .truncate()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_2)
                                .child(SharedString::from(window.label.clone())),
                        )
                        .child(quota_meter(window.used_percent, pace, tone, colors))
                        .child(
                            div()
                                .w(px(30.0))
                                .text_size(px(theme::FS_2XS))
                                .text_color(meter_tone(window.used_percent, tone, colors))
                                .child(SharedString::from(format!("{}%", window.used_percent))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from(reset.unwrap_or_default())),
                        ),
                );
            }
            panel = panel.child(section);
        }
        panel = panel.child(
            div()
                .pt(px(4.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .border_t_1()
                .border_color(colors.line)
                .child(
                    div()
                        .id("usage-refresh")
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_2)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(colors.text_1))
                        .child("Refresh")
                        .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                            // `quota.refresh` asks every provider for a new reading.
                            root.daemon_command(
                                crate::daemon::Command::Rpc {
                                    tag: "quota".to_owned(),
                                    method: "quota.refresh".to_owned(),
                                    params: serde_json::json!({}),
                                },
                                cx,
                            );
                            root.load_quotas(cx);
                        })),
                )
                .child(div().flex_1())
                .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(updated))),
        );
        Some(
            div()
                .absolute()
                .left(px(8.0))
                .bottom(px(64.0))
                .child(panel)
                .into_any_element(),
        )
    }

    /// The daemon's quota rows, parsed once per draw.
    pub(crate) fn quota_rows(&self) -> Vec<QuotaProvider> {
        self.snapshot().map(|snapshot| quota::rows(&snapshot.quotas)).unwrap_or_default()
    }

    /// Everything waiting on the owner.
    pub(crate) fn needs_items(&self) -> Vec<NeedsItem> {
        match self.snapshot() {
            Some(snapshot) => needs_items(&snapshot.permissions, &snapshot.sessions, &snapshot.agents),
            None => Vec::new(),
        }
    }

    /// Announce a turn that finished, failed or started waiting while the window
    /// is in the background. Only a real change of state is announced, and the
    /// first sight of a conversation is not a change.
    pub(crate) fn watch_notifications(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot.as_ref() else { return };
        let states: Vec<(String, String, Option<String>)> = snapshot
            .sessions
            .iter()
            .map(|session| (session.id.clone(), session.state_word().to_lowercase(), session.agent_id.clone()))
            .collect();
        if !self.watched_booted {
            self.watched_states = states.into_iter().map(|(id, state, _)| (id, state)).collect();
            self.watched_booted = true;
            return;
        }
        let mut announce: Vec<(String, String, crate::notify::Notice)> = Vec::new();
        for (id, state, agent_id) in states {
            let previous = self.watched_states.get(&id).cloned();
            self.watched_states.insert(id.clone(), state.clone());
            let Some(notice) = crate::notify::notice_for(previous.as_deref(), &state) else { continue };
            if self.focused {
                continue;
            }
            let level = agent_id
                .as_deref()
                .and_then(|agent_id| snapshot.agents.iter().find(|agent| agent.id == agent_id))
                .and_then(|agent| agent.notification_level.clone());
            let allowed = crate::notify::allowed(level.as_deref(), notice);
            if !allowed {
                continue;
            }
            let (bot_name, title) = match snapshot.session(&id) {
                Some(session) => (
                    session.agent_id.clone().and_then(|agent_id| snapshot.agent(&agent_id)).map(|agent| agent.name.clone()).unwrap_or_else(|| "Bitbots".to_owned()),
                    session.title_or_default().to_owned(),
                ),
                None => ("Bitbots".to_owned(), "Conversation".to_owned()),
            };
            announce.push((bot_name, title, notice));
        }
        for (bot_name, title, notice) in announce {
            if let Err(reason) = crate::notify::notify(&bot_name, &notice.body(&title)) {
                self.notice = Some(reason);
            }
        }
        let _ = cx;
    }
}

/// The thin meter: grey while calm, the working colour near a limit and the
/// danger colour at it; `pace` marks how much of the window's time has passed.
fn quota_meter(percent: f64, pace: Option<f64>, tone: QuotaTone, colors: Colors) -> AnyElement {
    let fill = match tone {
        QuotaTone::Normal => {
            let mut rgba = colors.text_1;
            rgba.a = 0.46;
            rgba
        }
        QuotaTone::Warning => colors.status_working,
        QuotaTone::Critical => colors.status_danger,
    };
    let width = percent.clamp(0.0, 100.0).max(2.0) as f32;
    let mut bar = div()
        .relative()
        .flex_1()
        .max_w(px(96.0))
        .h(px(3.0))
        .flex_none()
        .rounded(px(2.0))
        .bg(colors.selected)
        .child(div().h_full().w(px(width)).rounded(px(2.0)).bg(fill));
    if let Some(pace) = pace {
        bar = bar.child(div().absolute().top(px(-1.0)).h(px(5.0)).w(px(1.0)).bg(colors.line_strong).left(px(pace.clamp(0.0, 100.0) as f32)));
    }
    bar.into_any_element()
}

fn meter_tone(percent: f64, tone: QuotaTone, colors: Colors) -> gpui::Rgba {
    match (tone, percent) {
        (QuotaTone::Critical, _) => colors.status_danger,
        (QuotaTone::Warning, _) => colors.status_working,
        _ => colors.text_1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Agent, Permission, Session};

    fn permission(id: &str, session_id: &str, kind: Option<&str>, title: &str, created: &str) -> Permission {
        Permission {
            id: id.into(),
            session_id: Some(session_id.into()),
            kind: kind.map(str::to_owned),
            title: Some(title.into()),
            created_at: Some(created.into()),
            ..Permission::default()
        }
    }

    fn session(id: &str, agent_id: &str, archived: bool) -> Session {
        Session { id: id.into(), agent_id: Some(agent_id.into()), archived, ..Session::default() }
    }

    fn bot(id: &str, name: &str) -> Agent {
        Agent { id: id.into(), name: name.into(), ..Agent::default() }
    }

    #[test]
    fn the_list_names_the_bot_the_kind_and_the_words_oldest_first() {
        let bots = vec![bot("ada", "Ada")];
        let sessions = vec![session("s1", "ada", false)];
        let permissions = vec![
            permission("p2", "s1", None, "Run the tests", "2026-11-08T10:05:00Z"),
            permission("p1", "s1", Some("question"), "Which colour?", "2026-11-08T10:00:00Z"),
        ];
        let items = needs_items(&permissions, &sessions, &bots);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "Question");
        assert_eq!(items[0].summary, "Which colour?");
        assert_eq!(items[0].bot_name, "Ada");
        assert_eq!(items[1].kind, "Approval");
    }

    #[test]
    fn a_request_in_a_gone_or_archived_conversation_is_left_out() {
        let bots = vec![bot("ada", "Ada")];
        let sessions = vec![session("s1", "ada", true)];
        let permissions = vec![permission("p1", "s1", None, "Run the tests", "2026-11-08T10:00:00Z")];
        assert!(needs_items(&permissions, &sessions, &bots).is_empty());
        let other = vec![permission("p2", "gone", None, "Run the tests", "2026-11-08T10:00:00Z")];
        assert!(needs_items(&other, &sessions, &bots).is_empty());
    }

    #[test]
    fn a_long_sentence_is_clipped_and_flattened() {
        let text = format!("{} end", "word ".repeat(40));
        let clipped = clip(&text.split_whitespace().collect::<Vec<_>>().join(" "), 20);
        assert!(clipped.ends_with('…'));
        assert_eq!(clipped.chars().count(), 20);
    }

    #[test]
    fn the_waiting_time_reads_in_words() {
        let now = 1_000_000_000_000;
        assert_eq!(waited(Some(now - 10_000), now).as_deref(), Some("just now"));
        assert_eq!(waited(Some(now - 12 * 60_000), now).as_deref(), Some("12 min"));
        assert_eq!(waited(Some(now - 3 * 3_600_000), now).as_deref(), Some("3 h"));
        assert_eq!(waited(None, now), None);
    }
}
