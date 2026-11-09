//! `ui/ProposalCard.tsx`, for the two proposals this window can answer: a `hire`
//! from the CEO (one card per candidate, all of one `propose_hires` call sharing
//! a `groupId`) and a `bot` it proposes to create.
//!
//! Every other proposal, and every question, still says to open Retinue.
//! Calls `proposal.decide` with the proposal's own id, exactly as the React card.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::art::{self, Glyph, Mark};
use crate::state::{team, Permission};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

impl Root {
    /// The card for a waiting proposal. Every kind the daemon can carry gets a
    /// card with the decision it offers: `merge` also offers Discard, and a
    /// `task` that opens a conversation approves into it.
    pub(crate) fn render_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let kind = permission.proposal_kind().unwrap_or("").to_owned();
        match kind.as_str() {
            "hire" => self.render_hire_proposal(permission, colors, cx),
            "bot" => self.render_bot_proposal(permission, colors, cx),
            "project" => self.render_project_proposal(permission, colors, cx),
            "task" => self.render_task_proposal(permission, colors, cx),
            "playbook" => self.render_text_proposal(permission, "Update its playbook", "text", colors, cx),
            "profile" => self.render_text_proposal(permission, "Remember something about you", "value", colors, cx),
            "permission" => self.render_permission_proposal(permission, colors, cx),
            "merge" => self.render_merge_proposal(permission, colors, cx),
            _ => self.render_text_proposal(permission, "The CEO proposes", "reason", colors, cx),
        }
    }

    /// The shell every proposal card shares: who proposes, what it is, the body
    /// and the decisions.
    #[allow(clippy::too_many_arguments)]
    fn proposal_shell(
        &mut self,
        permission: &Permission,
        title: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        body: AnyElement,
        approve_label: &str,
        reject_label: &str,
        discard: bool,
    ) -> AnyElement {
        let ceo = self
            .snapshot()
            .and_then(|snapshot| snapshot.manager())
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "The CEO".to_owned());
        let proposal_id = permission.proposal.as_ref().map(|proposal| proposal.id.clone()).unwrap_or_default();
        let reject_id = proposal_id.clone();
        let discard_id = proposal_id.clone();
        let approve_id = proposal_id.clone();
        let mut actions = div().flex().items_center().justify_end().gap(px(8.0));
        if discard {
            actions = actions.child(self.ghost_button("Discard", &format!("discard-{proposal_id}"), colors, cx, move |root, cx| {
                root.decide_proposal_with(discard_id.clone(), "reject", true, cx);
            }));
        }
        actions = actions
            .child(self.ghost_button(reject_label, &format!("reject-{proposal_id}"), colors, cx, move |root, cx| {
                root.decide_proposal(reject_id.clone(), false, cx)
            }))
            .child(self.small_primary(approve_label, &format!("approve-{proposal_id}"), colors, cx, move |root, cx| {
                root.decide_proposal(approve_id.clone(), true, cx)
            }));
        div()
            .mx(px(12.0))
            .mb(px(8.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.line_strong)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(format!("{ceo} proposes"))))
                    .child(
                        div()
                            .text_size(px(theme::FS_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(title.to_owned())),
                    ),
            )
            .child(body)
            .child(actions)
            .into_any_element()
    }

    /// A labelled line inside a proposal card.
    fn proposal_field(&self, label: &str, value: &str, colors: Colors, mono: bool) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(label.to_owned())))
            .child(
                div()
                    .text_size(px(theme::FS_XS))
                    .text_color(colors.text_1)
                    .when(mono, |this| this.font_family(theme::MONO_FONT))
                    .whitespace_normal()
                    .child(SharedString::from(if value.trim().is_empty() { "—".to_owned() } else { value.to_owned() })),
            )
            .into_any_element()
    }

    fn render_project_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let folder = proposal.text("folder");
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.proposal_field("Name", proposal.text("name"), colors, false))
            .child(self.proposal_field("What it is for", proposal.text("description"), colors, false));
        body = body.child(self.proposal_field("Folder", folder, colors, true));
        if folder.trim().is_empty() {
            body = body.child(
                div()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child("The proposal names no folder, so the project keeps its conversations in the bot's own."),
            );
        }
        self.proposal_shell(permission, "Create a project", colors, cx, body.into_any_element(), "Approve", "Reject", false)
    }

    fn render_task_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.proposal_field("For", proposal.text("botName"), colors, false))
            .child(self.proposal_field("Project", proposal.text("projectName"), colors, false))
            .child(self.proposal_field("The task", proposal.text("brief"), colors, false))
            .child(self.proposal_field("Done when", proposal.text("doneGoal"), colors, false));
        self.proposal_shell(permission, "Hand a task to a bot", colors, cx, body.into_any_element(), "Approve", "Reject", false)
    }

    fn render_permission_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let mode = proposal.text("mode");
        let body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.proposal_field("Conversation", proposal.text("conversation"), colors, true))
            .child(self.proposal_field("Permission", if mode == "auto" { "Auto" } else { "Ask first" }, colors, false))
            .child(
                div()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child("Changes this bot's permission for all its conversations, from their next turn. Full access can never be set this way."),
            );
        self.proposal_shell(permission, "Change this bot's approvals", colors, cx, body.into_any_element(), "Approve", "Reject", false)
    }

    fn render_merge_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.proposal_field("Task", proposal.text("taskTitle"), colors, false))
            .child(self.proposal_field("Bot", proposal.text("botName"), colors, false))
            .child(self.proposal_field("Branch", proposal.text("branch"), colors, true))
            .child(self.proposal_field("Merge into", proposal.text("baseBranch"), colors, true))
            .child(self.proposal_field("Changes", proposal.text("diffSummary"), colors, false));
        let files: Vec<String> = proposal
            .payload
            .as_ref()
            .and_then(|payload| payload.get("files"))
            .and_then(|files| files.as_array())
            .map(|files| files.iter().filter_map(|file| file.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        if !files.is_empty() {
            body = body.child(self.proposal_field("Files", &files.join("\n"), colors, true));
        }
        body = body.child(
            div()
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child("The task ran in its own copy of the project so it could not clash with other work. Merging brings its changes back and removes the copy."),
        );
        self.proposal_shell(permission, "Merge its work back", colors, cx, body.into_any_element(), "Merge", "Keep branch", true)
    }

    /// A card whose body is one field: a playbook's text, a remembered fact, or a
    /// proposal this window cannot lay out.
    fn render_text_proposal(
        &mut self,
        permission: &Permission,
        title: &str,
        key: &str,
        colors: Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let mut body = div().flex().flex_col().gap(px(6.0));
        let reason = proposal.text("reason").to_owned();
        if !reason.is_empty() {
            body = body.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(reason)));
        }
        body = body.child(self.proposal_field(if key == "value" { "New value" } else { "Its proposal" }, proposal.text(key), colors, false));
        self.proposal_shell(permission, title, colors, cx, body.into_any_element(), "Approve", "Reject", false)
    }

    fn card_shell(&self, title: &str, subtitle: &str, colors: Colors) -> (gpui::Div, gpui::Div) {
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(subtitle.to_owned())))
                    .child(
                        div()
                            .text_size(px(theme::FS_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(title.to_owned())),
                    ),
            );
        let body = div().flex().flex_col();
        (head, body)
    }

    fn render_hire_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let nickname = proposal.text("nickname");
        let name = if nickname.is_empty() { proposal.text("botName") } else { nickname };
        let role = proposal.text("role");
        let portrait = proposal.text("portrait");
        let color = proposal
            .text("color")
            .to_owned()
            .pipe_if_empty(|| {
                team::presets()
                    .iter()
                    .find(|preset| preset.id == proposal.text("presetId"))
                    .map(|preset| preset.color.clone())
                    .unwrap_or_else(|| "violet".to_owned())
            });
        let project_name = proposal.text("projectName");
        let reason = proposal.text("reason");
        let siblings = team::hire_siblings(&self.permissions_snapshot(), permission).iter().map(|sibling| {
            sibling.proposal.as_ref().map(|proposal| proposal.id.clone()).unwrap_or_default()
        }).collect::<Vec<_>>();

        let ceo = self
            .snapshot()
            .and_then(|snapshot| snapshot.manager())
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "The CEO".to_owned());
        let mark = Mark::new(36.0, art::agent_color(&color), art::Icon::resolve(Some(portrait)), if name.is_empty() { "Employee" } else { name }, colors).render();
        let line = [role.to_owned(), if project_name.is_empty() { String::new() } else { format!("into {project_name}") }]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");

        let (head, body) = self.card_shell("Hire an employee", &format!("{ceo} proposes"), colors);
        let proposal_id = proposal.id.clone();
        let skip_id = proposal_id.clone();
        let mut actions = div().flex().items_center().justify_end().gap(px(8.0));
        if !siblings.is_empty() {
            let all: Vec<String> = std::iter::once(proposal_id.clone()).chain(siblings.iter().cloned()).collect();
            actions = actions.child(self.ghost_button(
                &format!("Hire all {}", all.len()),
                &format!("hire-all-{}", proposal_id),
                colors,
                cx,
                move |root, cx| root.decide_proposals(all.clone(), cx),
            ));
        }
        actions = actions
            .child(self.ghost_button("Skip", &format!("skip-{}", skip_id), colors, cx, move |root, cx| {
                root.decide_proposal(skip_id.clone(), false, cx)
            }))
            .child(self.small_primary("Hire", &format!("hire-{}", proposal_id), colors, cx, move |root, cx| {
                root.decide_proposal(proposal_id.clone(), true, cx)
            }));

        div()
            .mx(px(12.0))
            .mb(px(8.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.line_strong)
            .child(head)
            .child(
                body.flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(theme::FS_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(if name.is_empty() { "An employee".to_owned() } else { name.to_owned() })),
                            )
                            .when(!line.is_empty(), |this| {
                                this.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(line.clone())))
                            })
                            .when(!reason.is_empty(), |this| {
                                this.child(
                                    div()
                                        .mt(px(2.0))
                                        .text_size(px(theme::FS_XS))
                                        .text_color(colors.text_3)
                                        .child(SharedString::from(reason.to_owned())),
                                )
                            }),
                    ),
            )
            .when(!siblings.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(theme::FS_2XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(format!(
                            "{} more {} waiting after this one.",
                            siblings.len(),
                            if siblings.len() == 1 { "candidate is" } else { "candidates are" }
                        ))),
                )
            })
            .child(actions)
            .into_any_element()
    }

    fn render_bot_proposal(&mut self, permission: &Permission, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let Some(proposal) = permission.proposal.clone() else { return div().into_any_element() };
        let name = proposal.text("name");
        let role = proposal.text("role");
        let description = proposal.text("description");
        let provider = proposal.text("provider");
        let model = proposal.text("model");
        let agent_line = {
            let mut parts = vec![art::provider_name(provider)];
            if !model.is_empty() {
                parts.push(model.to_owned());
            }
            parts.into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
        };
        let ceo = self
            .snapshot()
            .and_then(|snapshot| snapshot.manager())
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "The CEO".to_owned());
        let proposal_id = proposal.id.clone();
        let skip_id = proposal_id.clone();
        let (head, body) = self.card_shell("Create a bot", &format!("{ceo} proposes"), colors);
        div()
            .mx(px(12.0))
            .mb(px(8.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.line_strong)
            .child(head)
            .child(
                body.flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(art::glyph(Glyph::USER_PLUS, 12.0, colors.text_3))
                            .child(div().text_size(px(theme::FS_SM)).font_weight(FontWeight::MEDIUM).child(SharedString::from(name.to_owned())))
                            .when(!role.is_empty(), |this| {
                                this.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from(role.to_owned())))
                            }),
                    )
                    .when(!description.is_empty(), |this| {
                        this.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from(description.to_owned())))
                    })
                    .when(!agent_line.is_empty(), |this| {
                        this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(agent_line.clone())))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .child(self.ghost_button("Skip", &format!("skip-{}", skip_id), colors, cx, move |root, cx| {
                        root.decide_proposal(skip_id.clone(), false, cx)
                    }))
                    .child(self.small_primary("Create", &format!("create-{}", proposal_id), colors, cx, move |root, cx| {
                        root.decide_proposal(proposal_id.clone(), true, cx)
                    })),
            )
            .into_any_element()
    }

    /// The waiting permissions, for the sibling lookup.
    fn permissions_snapshot(&self) -> Vec<Permission> {
        self.snapshot().map(|snapshot| snapshot.permissions.clone()).unwrap_or_default()
    }

    pub(crate) fn small_primary(
        &self,
        label: &str,
        id: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("primary-{id}")))
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(colors.accent)
            .text_color(colors.text_inverse)
            .text_size(px(theme::FS_XS))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected_strong))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }
}

/// A tiny convenience for "use this string, or ask for one when it is empty".
trait PipeEmpty {
    fn pipe_if_empty(self, fallback: impl FnOnce() -> String) -> String;
}

impl PipeEmpty for String {
    fn pipe_if_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}
