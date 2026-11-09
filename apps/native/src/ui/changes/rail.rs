//! The session rail: where the conversation works, how full its context is, what
//! it produced and what it changed, turn by turn. Ported from
//! `ui/SessionRail.tsx` and `ui/SessionRailExtras.tsx`.
//!
//! Every value is something the daemon reported. Git is the source of the
//! Changes card, so edits made by a command or by someone else show up too; the
//! per-turn list comes from the daemon's own checkpoints.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};
use serde_json::json;

use crate::state::changes::{diff_unavailable_reason, file_note};
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::schedules::describe_next_run;

/// How many changed files the rail lists before "Show all".
const CHANGE_LIMIT: usize = 8;

fn compact(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}k", value / 1_000.0)
    } else {
        format!("{}", value as i64)
    }
}

fn folder_of(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_owned()
}

impl Root {
    pub(crate) fn render_session_rail(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(session_id) = self.session_id.clone() else {
            return div()
                .w(px(288.0))
                .h_full()
                .flex_none()
                .border_l_1()
                .border_color(colors.line)
                .p(px(12.0))
                .text_size(px(theme::FS_XS))
                .text_color(colors.text_3)
                .child("Open a conversation to see where it works and what it changed.")
                .into_any_element();
        };
        let session = self.selected_session().cloned();
        let repository = self.changes.repository.get(&session_id).cloned();
        let environment = self.changes.environment.get(&session_id).cloned();
        let workspace = self.changes.workspace.get(&session_id).cloned();
        let checkpoints = self.changes.checkpoints.get(&session_id).cloned();
        let failed = self.changes.repository_failed.contains(&session_id);
        let view = crate::state::changes::build_rail_view(repository.as_ref());

        let mut rail = div()
            .id("session-rail")
            .w(px(288.0))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(10.0))
            .border_l_1()
            .border_color(colors.line)
            .overflow_y_scroll();

        // Environment.
        let project_path = repository
            .as_ref()
            .and_then(|repository| repository.project_path.clone())
            .or_else(|| environment.as_ref().and_then(|environment| environment.project_path.clone()))
            .or_else(|| session.as_ref().and_then(|session| session.project_path.clone()));
        let folder = session
            .as_ref()
            .and_then(|session| session.workspace_label.clone())
            .filter(|label| !label.is_empty())
            .or_else(|| project_path.as_deref().map(folder_of));
        let branch = repository
            .as_ref()
            .filter(|repository| repository.git_repo)
            .and_then(|repository| repository.branch.clone())
            .or_else(|| environment.as_ref().filter(|environment| environment.git_repo).and_then(|environment| environment.branch.clone()));
        let mut environment_body = div().flex().flex_col().gap(px(4.0));
        let mut any_environment = false;
        if let Some(folder) = folder.clone() {
            environment_body = environment_body.child(self.rail_row(folder, colors));
            any_environment = true;
        }
        if let Some(model) = session.as_ref().and_then(|session| session.model_lock.as_ref().and_then(|lock| lock.model.clone())) {
            environment_body = environment_body.child(self.rail_row(model, colors));
            any_environment = true;
        }
        if let Some(branch) = branch.clone() {
            let detached = repository.as_ref().is_some_and(|repository| repository.detached)
                || environment.as_ref().is_some_and(|environment| environment.detached);
            environment_body = environment_body.child(self.rail_row(
                format!("Branch {branch}{}", if detached { " (detached)" } else { "" }),
                colors,
            ));
            any_environment = true;
        }
        if !any_environment {
            environment_body = environment_body.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No folder yet."));
        }
        rail = rail.child(self.rail_card("environment", "Environment", None, environment_body.into_any_element(), colors, cx));

        // Workspace: its own copy, or the shared folder.
        let mut workspace_body = div().flex().flex_col().gap(px(4.0));
        match workspace.as_ref() {
            None => {
                workspace_body = workspace_body.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("Reading the workspace…"));
            }
            Some(workspace) => {
                let own = workspace.mode == "isolated" && workspace.worktree.is_some();
                let label = if own {
                    "Its own copy of the project".to_owned()
                } else if workspace.mode == "unknown" {
                    "No project folder".to_owned()
                } else {
                    "Shared project folder".to_owned()
                };
                workspace_body = workspace_body.child(self.rail_row(format!("{label} · {}", if own { "isolated" } else { "shared" }), colors));
                if let Some(worktree) = workspace.worktree.as_ref() {
                    workspace_body = workspace_body.child(note(colors, format!(
                        "Branch {}, taken from {}. Changes here are this bot's alone.",
                        worktree.branch, worktree.base
                    )));
                }
                if let Some(failure) = workspace.isolation_failure.clone() {
                    workspace_body = workspace_body.child(note(colors, format!("Could not give this bot its own copy: {failure} It is working in the shared folder.")));
                }
                if !workspace.shared_with.is_empty() {
                    let count = workspace.shared_with.len();
                    workspace_body = workspace_body.child(note(colors, format!(
                        "{count} other {} working in this folder right now, so changes can overlap.",
                        if count == 1 { "conversation is" } else { "conversations are" }
                    )));
                }
            }
        }
        rail = rail.child(self.rail_card("workspace", "Workspace", None, workspace_body.into_any_element(), colors, cx));

        // Schedules: this bot's own automations, soonest first.
        let now_ms = chrono::Utc::now().timestamp_millis();
        let agent_id = session.as_ref().and_then(|session| session.agent_id.clone());
        let mut upcoming: Vec<crate::ui::schedules::Automation> = self
            .schedules
            .automations
            .iter()
            .filter(|automation| agent_id.as_deref().map(|id| automation.agent_id == id).unwrap_or(false))
            .cloned()
            .collect();
        upcoming.sort_by(|a, b| a.next_run_at.clone().unwrap_or_default().cmp(&b.next_run_at.clone().unwrap_or_default()));
        let mut schedule_body = div().flex().flex_col().gap(px(4.0));
        if upcoming.is_empty() {
            schedule_body = schedule_body.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child("No schedule for this bot."));
        }
        for automation in upcoming.iter().take(4) {
            let when = match automation.next_run_at.as_deref() {
                Some(next) if automation.enabled => format!("Next run {}", describe_next_run(Some(next), now_ms)),
                _ => automation.state_line(now_ms),
            };
            schedule_body = schedule_body.child(note(colors, format!("{} · {when}", automation.name)));
        }
        schedule_body = schedule_body.child(
            div()
                .id("rail-open-schedules")
                .h(px(24.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .rounded(px(theme::RADIUS_SM))
                .text_size(px(theme::FS_XS))
                .text_color(colors.text_2)
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .child("Open Schedules")
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_schedules(cx))),
        );
        rail = rail.child(self.rail_card("schedules", "Schedules", None, schedule_body.into_any_element(), colors, cx));

        // Context.
        let used = session.as_ref().and_then(|session| session.context_used).unwrap_or(0.0);
        let model = session.as_ref().and_then(|session| session.context_model.clone());
        let mut context_body = div().flex().flex_col().gap(px(4.0));
        if used > 0.0 {
            context_body = context_body.child(self.rail_row(format!("{} tokens used", compact(used)), colors));
            if let Some(model) = model {
                context_body = context_body.child(note(colors, format!("Measured on {model}")));
            }
            context_body = context_body.child(note(
                colors,
                "The coding agent reports how much of the window it used, not the window's size, so there is no meter here.".to_owned(),
            ));
        } else {
            context_body = context_body.child(note(colors, "The coding agent has not reported this conversation's context yet.".to_owned()));
        }
        rail = rail.child(self.rail_card("context", "Context", None, context_body.into_any_element(), colors, cx));

        // Outputs: files Git lists as new.
        if !view.created.is_empty() {
            let mut outputs = div().flex().flex_col().gap(px(2.0));
            for file in view.created.iter().take(5) {
                outputs = outputs.child(self.rail_file_row(file.name.clone(), file.relative.clone(), None, None, colors));
            }
            rail = rail.child(self.rail_card(
                "outputs",
                "Outputs",
                Some(SharedString::from(view.created.len().to_string())),
                outputs.into_any_element(),
                colors,
                cx,
            ));
        }

        // Changes: Git's own list against HEAD.
        let total = view.repository.len();
        let aside = (total > 0).then(|| SharedString::from(format!("+{} −{}", view.added, view.removed)));
        let mut changes = div().flex().flex_col().gap(px(4.0));
        if failed {
            changes = changes.child(note(colors, "The repository could not be read just now. These changes may be out of date.".to_owned()));
        } else if let Some(repository) = repository.as_ref() {
            if !repository.git_repo {
                changes = changes.child(note(colors, format!(
                    "{} is not a Git repository, so changes cannot be tracked.",
                    folder.clone().unwrap_or_else(|| "This folder".to_owned())
                )));
            } else if let Some(error) = repository.error.clone() {
                changes = changes.child(note(colors, format!("Git could not be read here: {error}")));
            } else if repository.files.is_empty() {
                changes = changes.child(note(colors, "No uncommitted changes.".to_owned()));
            } else {
                changes = changes.child(note(
                    colors,
                    format!(
                        "All uncommitted changes in {}, not only this conversation's.",
                        repository.repo_root.as_deref().map(folder_of).unwrap_or_else(|| "this repository".to_owned())
                    ),
                ));
            }
        } else {
            changes = changes.child(note(colors, "Reading the repository…".to_owned()));
        }
        if view.from_repository && total > 0 {
            let mut actions = div().flex().gap(px(4.0));
            actions = actions.child(
                div()
                    .id("rail-review")
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child("Review")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_changes_view(None, cx))),
            );
            actions = actions.child(
                div()
                    .id("rail-create-pr")
                    .h(px(24.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.raised)
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_2)
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.selected))
                    .child("Create PR")
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.open_pull_request(cx))),
            );
            changes = changes.child(actions);
        }
        let show_all = total <= CHANGE_LIMIT;
        for file in view.repository.iter().take(if show_all { total } else { CHANGE_LIMIT }) {
            let name = file.name.clone();
            let relative = file.relative.clone();
            let note = file_note(file).map(str::to_owned);
            let reason = diff_unavailable_reason(file);
            let reviewable = file.source == "repository" && file.relative.is_some();
            let row = self.rail_file_row(name, relative.clone(), Some((file.added, file.removed)), note, colors);
            let target = relative.clone();
            let row = div()
                .id(SharedString::from(format!("rail-file-{}", file.key)))
                .rounded(px(theme::RADIUS_SM))
                // Anything Git lists opens in the review screen, which reads the
                // whole diff itself; a row it cannot show says why in its title.
                .when(reviewable && reason.is_none(), |this| {
                    this.cursor_pointer()
                        .hover(move |style| style.bg(colors.hover))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            root.open_changes_view(target.clone(), cx);
                        }))
                })
                .child(row);
            changes = changes.child(row);
        }
        rail = rail.child(self.rail_card("changes", "Changes", aside, changes.into_any_element(), colors, cx));

        // Changes by turn: the daemon's checkpoints, newest first.
        let mut turns_body = div().flex().flex_col().gap(px(4.0));
        match checkpoints.as_ref() {
            None => turns_body = turns_body.child(note(colors, "Checking what each turn changed…".to_owned())),
            Some(checkpoints) if checkpoints.turns.is_empty() => {
                turns_body = turns_body.child(note(colors, "Files the coding agent changes appear here, turn by turn.".to_owned()))
            }
            Some(checkpoints) => {
                if !checkpoints.capability.file_undo {
                    turns_body = turns_body.child(note(colors, checkpoints.capability.file_undo_reason.clone().unwrap_or_else(|| "Single changes cannot be undone here.".to_owned())));
                } else if checkpoints.busy {
                    turns_body = turns_body.child(note(colors, "Undo is available when the turn finishes.".to_owned()));
                }
                let mut turns = checkpoints.turns.clone();
                turns.reverse();
                for turn in turns {
                    let added = checkpoints.turn_added(&turn);
                    let removed = checkpoints.turn_removed(&turn);
                    let index = session
                        .as_ref()
                        .and_then(|session| session.turns.iter().position(|candidate| Some(&candidate.id) == turn.turn_id.as_ref()));
                    let title = match index {
                        Some(index) => format!("Turn {} · +{added} −{removed}", index + 1),
                        None => format!("Earlier changes · +{added} −{removed}"),
                    };
                    let mut block = div().flex().flex_col().gap(px(2.0)).child(
                        div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).font_weight(FontWeight::MEDIUM).child(SharedString::from(title)),
                    );
                    for file in &turn.files {
                        let name = file.path.replace('\\', "/").rsplit('/').next().unwrap_or(&file.path).to_owned();
                        let can_undo = file.state == "available" && checkpoints.can_undo();
                        let confirming = self.changes.undo_confirm.as_deref() == Some(file.id.as_str());
                        let file_id = file.id.clone();
                        let mut row = div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(theme::FS_2XS))
                            .child(div().flex_1().min_w(px(0.0)).truncate().text_color(colors.text_2).child(SharedString::from(name)))
                            .when(file.state != "available", |this| {
                                this.child(div().text_color(colors.text_3).child(if file.state == "reverted" { "undone" } else { "cannot undo" }))
                            });
                        if can_undo {
                            let session_id = session_id.clone();
                            row = row.child(
                                div()
                                    .id(SharedString::from(format!("rail-undo-{}", file.id)))
                                    .px(px(4.0))
                                    .rounded(px(theme::RADIUS_SM))
                                    .cursor_pointer()
                                    .text_color(colors.text_3)
                                    .hover(move |style| style.text_color(colors.text_1).bg(colors.hover))
                                    .child(if confirming { "Undo this change" } else { "Undo" })
                                    .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                        if root.changes.undo_confirm.as_deref() != Some(file_id.as_str()) {
                                            root.changes.undo_confirm = Some(file_id.clone());
                                            cx.notify();
                                            return;
                                        }
                                        root.changes.undo_confirm = None;
                                        root.daemon_command(
                                            crate::daemon::Command::Rpc {
                                                tag: "file-revert".to_owned(),
                                                method: "file.revert".to_owned(),
                                                params: json!({ "sessionId": session_id, "fileChangeId": file_id }),
                                            },
                                            cx,
                                        );
                                        cx.notify();
                                    })),
                            );
                        }
                        block = block.child(row);
                    }
                    turns_body = turns_body.child(block);
                }
            }
        }
        rail = rail.child(self.rail_card("checkpoints", "Changes by turn", None, turns_body.into_any_element(), colors, cx));

        // Delegations: what this conversation handed to teammates. React draws
        // these inline in the conversation; the rail is where this window can put
        // them (the conversation pane is the Director's file).
        let delegations = self.delegations.for_session(&session_id);
        if !delegations.is_empty() {
            let summary = crate::ui::delegation::summarize(&delegations);
            let mut rows = div().flex().flex_col().gap(px(4.0));
            for record in &delegations {
                let side = record.side_session_id.clone();
                let side_session = self.snapshot().and_then(|snapshot| snapshot.session(&side)).cloned();
                let asking = self
                    .snapshot()
                    .and_then(|snapshot| snapshot.pending_permission(&record.side_session_id))
                    .is_some();
                let label = record.label(side_session.as_ref(), asking);
                let agent = record
                    .target_agent_id
                    .as_deref()
                    .and_then(|id| self.snapshot().and_then(|snapshot| snapshot.agent(id)))
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| "A teammate".to_owned());
                let elapsed = record.elapsed(now_ms);
                let stop_id = record.id.clone();
                let active = record.is_active();
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(theme::FS_2XS))
                            .child(div().flex_1().min_w(px(0.0)).truncate().text_color(colors.text_2).child(SharedString::from(format!("{agent} · {}", record.topic))))
                            .child(div().text_color(colors.text_3).child(SharedString::from(elapsed)))
                            .when(active, |this| {
                                this.child(
                                    div()
                                        .id(SharedString::from(format!("delegation-stop-{}", record.id)))
                                        .px(px(4.0))
                                        .rounded(px(theme::RADIUS_SM))
                                        .cursor_pointer()
                                        .text_color(colors.text_3)
                                        .hover(move |style| style.text_color(colors.status_danger).bg(colors.hover))
                                        .child("Stop")
                                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                                            root.cancel_delegation(stop_id.clone(), cx);
                                        })),
                                )
                            }),
                    )
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).truncate().child(SharedString::from(label)));
                if let Some(error) = record.error.clone() {
                    row = row.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).truncate().child(SharedString::from(error)));
                }
                rows = rows.child(row);
            }
            rail = rail.child(self.rail_card("delegations", "Delegations", Some(SharedString::from(summary)), rows.into_any_element(), colors, cx));
        }

        rail.into_any_element()
    }

    /// One rail card: a header that folds it, and its body.
    fn rail_card(
        &mut self,
        id: &str,
        title: &str,
        aside: Option<SharedString>,
        body: AnyElement,
        colors: Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = !self.changes.rail_collapsed.contains(id);
        let key = id.to_owned();
        let mut header = div()
            .id(SharedString::from(format!("rail-head-{id}")))
            .h(px(24.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(theme::FS_XS))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.text_2)
                    .child(SharedString::from(format!("{} {title}", if open { "▾" } else { "▸" }))),
            )
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                if !root.changes.rail_collapsed.remove(&key) {
                    root.changes.rail_collapsed.insert(key.clone());
                }
                cx.notify();
            }));
        if let Some(aside) = aside {
            header = header.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(aside));
        }
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pb(px(4.0))
            .border_b_1()
            .border_color(colors.line)
            .child(header);
        if open {
            card = card.child(body);
        }
        card.into_any_element()
    }

    /// One file row in the rail: its name, its counts and its rule.
    fn rail_file_row(
        &self,
        name: String,
        relative: Option<String>,
        counts: Option<(Option<i64>, Option<i64>)>,
        note: Option<String>,
        colors: Colors,
    ) -> AnyElement {
        let mut row = div()
            .h(px(22.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(theme::FS_XS))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(colors.text_2)
                    .child(SharedString::from(name)),
            )
            .when_some(note, |this, note| {
                this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(note)))
            });
        if let Some((added, removed)) = counts {
            if let Some(added) = added.filter(|value| *value > 0) {
                row = row.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_success).child(SharedString::from(format!("+{added}"))));
            }
            if let Some(removed) = removed.filter(|value| *value > 0) {
                row = row.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(format!("−{removed}"))));
            }
        }
        let _ = relative;
        row.into_any_element()
    }

}

fn note(colors: Colors, text: String) -> AnyElement {
    div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(text)).into_any_element()
}
