//! Bots: the "Create your own employee" wizard, a bot's own settings, and the
//! archived list (`ui/BotWizard.tsx`, `ui/BlobSettings.tsx`,
//! `ui/LookEditor.tsx` as far as Haze goes, `ui/ArchivedBots.tsx`).
//!
//! The wizard and the settings page are one form (`state::bot::BotForm`) shown
//! over the window, so a field is written once and both screens have it. The look
//! is Haze's own: a logo in one of three treatments, or a character portrait, on
//! the bot's colour.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString, Window};

use crate::art::{self, Glyph, Mark};
use crate::state::bot::{self, BotForm, STEPS};
use crate::state::{Agent, APPROVAL_MODES};
use crate::state::approval_mode_copy;
use crate::theme::{self, Colors};
use crate::ui::root::Root;

/// The colour swatches every bot may wear (`AGENT_SWATCHES`).
pub const SWATCHES: [(&str, u32); 12] = [
    ("coral", 0xF38C6F),
    ("orange", 0xF0A05A),
    ("amber", 0xE8C15A),
    ("lemon", 0xE4D56A),
    ("lime", 0xA8D45C),
    ("mint", 0x89D6B3),
    ("teal", 0x5EBEB0),
    ("cyan", 0x6EC8E0),
    ("sky", 0x7EB6F0),
    ("blue", 0x82AAFF),
    ("violet", 0xBF9CFF),
    ("pink", 0xF0A0C4),
];

/// The three Haze logo treatments, in the app's order.
pub const LOGOS: [(&str, &str); 3] = [("logo", "Logo"), ("logo-tint", "Logo in its colour"), ("logo-mono", "Logo in one colour")];

impl Root {
    /// Open the wizard for a new bot.
    pub(crate) fn open_bot_wizard(&mut self, cx: &mut Context<Self>) {
        let runtime_id = self
            .snapshot()
            .map(|snapshot| snapshot.usable_runtimes().first().map(|runtime| runtime.id.clone()).unwrap_or_default())
            .unwrap_or_default();
        let form = BotForm { runtime_id, ..BotForm::creating() };
        self.fill_bot_fields(&form, cx);
        self.request_catalog(form.runtime_id.clone(), cx);
        self.bot_form = Some(form);
        self.pending_focus = Some(crate::ui::root::FocusTarget::BotName);
        cx.notify();
    }

    /// Open one bot's own settings.
    pub(crate) fn open_bot_editor(&mut self, agent_id: String, cx: &mut Context<Self>) {
        let Some(agent) = self.snapshot().and_then(|snapshot| snapshot.agent(&agent_id)).cloned() else { return };
        let mut form = BotForm::editing(agent.id.clone());
        form.name = agent.name.clone();
        form.role = agent.role.clone().unwrap_or_default();
        form.description = agent.description.clone();
        form.instructions = agent.instructions.clone().unwrap_or_default();
        form.tags = agent.tags.join(", ");
        form.runtime_id = agent.runtime_id.clone();
        form.model = agent.model.clone();
        form.thinking = agent.thinking.clone();
        form.approval_mode = agent.permission_mode();
        form.project_id = agent.project_id.clone();
        form.icon = agent.icon().unwrap_or("logo").to_owned();
        form.color = agent.color.clone();
        self.fill_bot_fields(&form, cx);
        self.request_catalog(form.runtime_id.clone(), cx);
        self.bot_form = Some(form);
        self.pending_focus = Some(crate::ui::root::FocusTarget::BotName);
        cx.notify();
    }

    pub(crate) fn close_bot_form(&mut self, cx: &mut Context<Self>) {
        self.bot_form = None;
        cx.notify();
    }

    /// Put a form's values into the fields the window edits.
    pub(crate) fn fill_bot_fields(&mut self, form: &BotForm, cx: &mut Context<Self>) {
        let fields = [
            (self.bot_name.clone(), form.name.clone()),
            (self.bot_role.clone(), form.role.clone()),
            (self.bot_description.clone(), form.description.clone()),
            (self.bot_instructions.clone(), form.instructions.clone()),
            (self.bot_tags.clone(), form.tags.clone()),
        ];
        for (input, text) in fields {
            input.update(cx, |input, cx| input.set_text(text, cx));
        }
    }

    /// Read the fields back into the form before checking or saving it.
    pub(crate) fn read_bot_fields(&mut self, cx: &mut Context<Self>) {
        let name = self.bot_name.read(cx).text().to_owned();
        let role = self.bot_role.read(cx).text().to_owned();
        let description = self.bot_description.read(cx).text().to_owned();
        let instructions = self.bot_instructions.read(cx).text().to_owned();
        let tags = self.bot_tags.read(cx).text().to_owned();
        if let Some(form) = self.bot_form.as_mut() {
            form.name = name;
            form.role = role;
            form.description = description;
            form.instructions = instructions;
            form.tags = tags;
        }
    }

    /// Save the form: `agent.create` for a new bot, `agent.update` for an edit.
    pub(crate) fn save_bot(&mut self, cx: &mut Context<Self>) {
        self.read_bot_fields(cx);
        let Some(form) = self.bot_form.clone() else { return };
        let errors = bot::validate(&form);
        if !errors.is_empty() {
            let first = errors.values().next().cloned().unwrap_or_default();
            if let Some(form) = self.bot_form.as_mut() {
                form.step = 0;
            }
            self.error = Some(first);
            cx.notify();
            return;
        }
        let params = bot::params(&form);
        match form.agent_id.clone() {
            Some(agent_id) => {
                let mut params = params;
                params["agentId"] = serde_json::json!(agent_id);
                self.daemon_command(
                    crate::daemon::Command::Rpc { tag: "agent-save".to_owned(), method: "agent.update".to_owned(), params },
                    cx,
                );
            }
            None => {
                self.daemon_command(
                    crate::daemon::Command::Rpc { tag: "agent-create".to_owned(), method: "agent.create".to_owned(), params },
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Archive a bot (`agent.delete`), after the owner agrees.
    pub(crate) fn archive_bot(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "agent-archive".to_owned(),
                method: "agent.delete".to_owned(),
                params: serde_json::json!({ "agentId": agent_id }),
            },
            cx,
        );
        self.bot_form = None;
        cx.notify();
    }

    /// Bring an archived bot back (`agent.restore`).
    pub(crate) fn restore_bot(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.daemon_command(
            crate::daemon::Command::Rpc {
                tag: "agent-restore".to_owned(),
                method: "agent.restore".to_owned(),
                params: serde_json::json!({ "agentId": agent_id }),
            },
            cx,
        );
    }

    /// The wizard or the bot's settings, over the window.
    pub(crate) fn render_bot_dialog(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.bot_form.clone()?;
        let editing = form.is_edit();
        let errors = bot::validate(&form);
        let is_wizard = !editing;
        let step = form.step.min(STEPS.len() - 1);

        let head = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(theme::FS_MD))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(if editing { format!("{} settings", form.name) } else { "Create your own employee".to_owned() })),
                    )
                    .child(
                        div()
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(if is_wizard { STEPS[step].to_owned() } else { "Everything this bot is and may do.".to_owned() })),
                    ),
            )
            .child(
                div()
                    .id("bot-close")
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .child(art::glyph(Glyph::X, 14.0, colors.text_3))
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.close_bot_form(cx))),
            );

        let mut body = div().flex().flex_col().gap(px(10.0)).child(head);
        if is_wizard {
            let mut steps = div().flex().gap(px(4.0)).flex_wrap();
            for (index, name) in STEPS.iter().enumerate() {
                let done = index < step;
                let on = index == step;
                steps = steps.child(
                    div()
                        .id(SharedString::from(format!("bot-step-{index}")))
                        .h(px(22.0))
                        .px(px(7.0))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(theme::RADIUS_SM))
                        .bg(if on { colors.selected } else { colors.hover })
                        .text_size(px(theme::FS_2XS))
                        .text_color(if on { colors.text_1 } else { colors.text_3 })
                        .cursor_pointer()
                        .hover(move |style| style.text_color(colors.text_1))
                        .child(SharedString::from(if done { format!("✓ {name}") } else { format!("{}. {name}", index + 1) }))
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(form) = root.bot_form.as_mut() {
                                form.step = index;
                            }
                            cx.notify();
                        })),
                );
            }
            body = body.child(steps);
        }

        // The look preview beside the fields.
        let provider = self
            .selected_runtime_provider(&form.runtime_id)
            .or_else(|| self.snapshot().map(|snapshot| snapshot.runtimes.iter().find(|runtime| runtime.id == form.runtime_id).map(|runtime| runtime.provider.clone()).unwrap_or_default()));
        let mark = Mark::for_agent(64.0, art::agent_color(&form.color), Some(&form.icon), provider.as_deref(), form.name.clone(), colors).render();

        let show = |step_index: usize, field: bool| !is_wizard || step == step_index || field;
        let mut column = div().flex().flex_col().gap(px(10.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(mark)
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(self.labeled_input("Name", &self.bot_name.clone(), colors, errors.get("name").cloned()))
                        .child(self.labeled_input("Role", &self.bot_role.clone(), colors, errors.get("role").cloned())),
                ),
        );
        if show(1, true) {
            column = column.child(self.labeled_input("What it does", &self.bot_description.clone(), colors, errors.get("description").cloned()));
        }
        // Which coding agent, and which of its models.
        if show(2, true) {
            let mut runtimes = div().flex().flex_wrap().gap(px(4.0));
            let usable = self
                .snapshot()
                .map(|snapshot| {
                    snapshot
                        .usable_runtimes()
                        .into_iter()
                        .map(|runtime| (runtime.id.clone(), art::provider_name(&runtime.provider)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for (id, label) in &usable {
                let id = id.clone();
                runtimes = runtimes.child(self.choice_chip(label, form.runtime_id == *id, &format!("bot-runtime-{id}"), colors, cx, move |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.runtime_id = id.clone();
                        form.model = None;
                        form.thinking = None;
                    }
                    root.request_catalog(id.clone(), cx);
                    cx.notify();
                }));
            }
            if usable.is_empty() {
                runtimes = runtimes.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child("No coding agent is set up yet."));
            }
            column = column.child(self.field_label("Coding agent", errors.get("runtime").cloned(), runtimes, colors));
            let mut models = div().flex().flex_wrap().gap(px(4.0));
            models = models.child(self.choice_chip("Agent default", form.model.is_none(), "bot-model-default", colors, cx, |root, cx| {
                if let Some(form) = root.bot_form.as_mut() {
                    form.model = None;
                    form.thinking = None;
                }
                cx.notify();
            }));
            let catalog = self.catalogs.get(&form.runtime_id).cloned().unwrap_or_default();
            for choice in catalog.models.iter().take(12) {
                let id = choice.id.clone();
                let label = choice.label.clone();
                models = models.child(self.choice_chip(
                    &label,
                    form.model.as_deref() == Some(id.as_str()),
                    &format!("bot-model-{id}"),
                    colors,
                    cx,
                    move |root, cx| {
                        if let Some(form) = root.bot_form.as_mut() {
                            form.model = Some(id.clone());
                            form.thinking = None;
                        }
                        cx.notify();
                    },
                ));
            }
            if catalog.models.is_empty() {
                models = models.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("The model list loads when the runtime answers."));
            }
            column = column.child(self.field_label("Model", None, models, colors));
            // Effort, when this model has levels.
            let levels = catalog.model(form.model.as_deref()).map(|choice| choice.thinking.clone()).unwrap_or_default();
            if !levels.is_empty() {
                let mut efforts = div().flex().flex_wrap().gap(px(4.0));
                efforts = efforts.child(self.choice_chip("Model default", form.thinking.is_none(), "bot-effort-default", colors, cx, |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.thinking = None;
                    }
                    cx.notify();
                }));
                for level in &levels {
                    let level = level.clone();
                    let label = crate::ui::composer::effort_label(&level);
                    efforts = efforts.child(self.choice_chip(
                        &label,
                        form.thinking.as_deref() == Some(level.as_str()),
                        &format!("bot-effort-{level}"),
                        colors,
                        cx,
                        move |root, cx| {
                            if let Some(form) = root.bot_form.as_mut() {
                                form.thinking = Some(level.clone());
                            }
                            cx.notify();
                        },
                    ));
                }
                column = column.child(self.field_label("Effort", None, efforts, colors));
            }
        }
        if show(2, true) {
            column = column.child(self.labeled_input(
                "Instructions",
                &self.bot_instructions.clone(),
                colors,
                errors.get("instructions").cloned(),
            ));
        }
        // Access: the permission mode.
        if show(3, true) {
            let mut modes = div().flex().flex_wrap().gap(px(4.0));
            for mode in APPROVAL_MODES {
                let (label, detail) = approval_mode_copy(mode);
                let value = mode.to_owned();
                modes = modes.child(self.choice_chip(&label, form.approval_mode == mode, &format!("bot-mode-{mode}"), colors, cx, move |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.approval_mode = value.clone();
                    }
                    cx.notify();
                }).into_any_element());
                let _ = detail;
            }
            column = column.child(self.field_label("Permission", None, modes, colors));
            // The look: a logo or a portrait, and the colour.
            let mut looks = div().flex().flex_wrap().gap(px(4.0));
            for (icon, label) in LOGOS {
                let value = icon.to_owned();
                looks = looks.child(self.choice_chip(label, form.icon == icon, &format!("bot-icon-{icon}"), colors, cx, move |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.icon = value.clone();
                    }
                    cx.notify();
                }));
            }
            for portrait in art::portrait_ids() {
                let value = (*portrait).to_owned();
                let selected = form.icon == *portrait;
                looks = looks.child(
                    div()
                        .id(SharedString::from(format!("bot-icon-{portrait}")))
                        .size(px(30.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(theme::RADIUS_SM))
                        .border_1()
                        .border_color(if selected { colors.text_1 } else { colors.line })
                        .cursor_pointer()
                        .hover(move |style| style.border_color(colors.line_strong))
                        .child(Mark::new(24.0, art::agent_color(&form.color), art::Icon::resolve(Some(portrait)), "Portrait", colors).render())
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(form) = root.bot_form.as_mut() {
                                form.icon = value.clone();
                            }
                            cx.notify();
                        })),
                );
            }
            column = column.child(self.field_label("Portrait or logo", errors.get("icon").cloned(), looks, colors));
            let mut swatches = div().flex().flex_wrap().gap(px(4.0));
            for (name, hex) in SWATCHES {
                let value = name.to_owned();
                let selected = form.color.eq_ignore_ascii_case(name);
                swatches = swatches.child(
                    div()
                        .id(SharedString::from(format!("bot-colour-{name}")))
                        .size(px(20.0))
                        .rounded_full()
                        .bg(gpui::rgb(hex))
                        .border_2()
                        .border_color(if selected { colors.text_1 } else { gpui::rgba(0x00000000) })
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(form) = root.bot_form.as_mut() {
                                form.color = value.clone();
                            }
                            cx.notify();
                        })),
                );
            }
            column = column.child(self.field_label("Colour", errors.get("color").cloned(), swatches, colors));
            column = column.child(self.labeled_input("Tags", &self.bot_tags.clone(), colors, None));
        }

        if let Some(error) = self.error.clone().filter(|_| self.bot_form.is_some()) {
            column = column.child(div().text_size(px(theme::FS_XS)).text_color(colors.status_danger).child(SharedString::from(error)));
        }
        body = body.child(column);

        // Footer: the wizard's steps, or a bot's own save and archive.
        let mut footer = div().flex().items_center().gap(px(6.0));
        if is_wizard {
            if step > 0 {
                footer = footer.child(self.ghost_button("Back", "bot-back", colors, cx, |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.step = form.step.saturating_sub(1);
                    }
                    cx.notify();
                }));
            }
            footer = footer.child(div().flex_1());
            if step + 1 < STEPS.len() {
                let blocker = bot::step_blocker(&form);
                footer = footer.child(self.ghost_button("Show all settings", "bot-all", colors, cx, |root, cx| {
                    if let Some(form) = root.bot_form.as_mut() {
                        form.step = STEPS.len() - 1;
                    }
                    cx.notify();
                }));
                let blocked = blocker.is_some();
                footer = footer.child(
                    div()
                        .id("bot-next")
                        .h(px(28.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(theme::RADIUS_SM))
                        .bg(if blocked { colors.raised } else { colors.accent })
                        .text_color(if blocked { colors.text_3 } else { colors.text_inverse })
                        .text_size(px(theme::FS_XS))
                        .font_weight(FontWeight::MEDIUM)
                        .when(!blocked, |this| {
                            this.cursor_pointer().hover(move |style| style.bg(colors.selected_strong)).on_click(cx.listener(
                                |root, _: &gpui::ClickEvent, _window, cx| {
                                    if let Some(form) = root.bot_form.as_mut() {
                                        form.step = (form.step + 1).min(STEPS.len() - 1);
                                    }
                                    cx.notify();
                                },
                            ))
                        })
                        .child(if let Some(blocker) = blocker { blocker } else { "Next".to_owned() }),
                );
            } else {
                footer = footer.child(self.small_primary("Create bot", "bot-create", colors, cx, |root, cx| root.save_bot(cx)));
            }
        } else {
            footer = footer
                .child(self.ghost_button("Archive", "bot-archive", colors, cx, |root, cx| {
                    if let Some(form) = root.bot_form.clone() {
                        if let Some(agent_id) = form.agent_id {
                            root.archive_bot(agent_id, cx);
                        }
                    }
                }))
                .child(div().flex_1())
                .child(self.small_primary("Save", "bot-save", colors, cx, |root, cx| root.save_bot(cx)));
        }
        body = body.child(footer);

        let card = div()
            .id("bot-card")
            .w(px(600.0))
            .max_h(px(620.0))
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .rounded(px(theme::RADIUS_LG))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .child(body);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.scrim)
                .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                    root.close_bot_form(cx);
                }))
                .child(div().on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                            // A click inside the card is not a click on the backdrop.
                            cx.stop_propagation();
                        })).child(card))
                .into_any_element(),
        )
    }

    fn selected_runtime_provider(&self, runtime_id: &str) -> Option<String> {
        self.snapshot()
            .and_then(|snapshot| snapshot.runtimes.iter().find(|runtime| runtime.id == runtime_id))
            .map(|runtime| runtime.provider.clone())
    }

    /// A field with its own name above the box, and its problem under it.
    fn labeled_input(&self, label: &str, input: &gpui::Entity<crate::ui::text_input::TextInput>, colors: Colors, error: Option<String>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).child(SharedString::from(label.to_owned())))
            .child(
                div()
                    .min_h(px(28.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme::RADIUS_SM))
                    .bg(colors.input)
                    .border_1()
                    .border_color(if error.is_some() { colors.status_danger } else { colors.line })
                    .child(input.clone()),
            )
            .when_some(error, |this, error| {
                this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(error)))
            })
            .into_any_element()
    }

    fn field_label(&self, label: &str, error: Option<String>, body: gpui::Div, colors: Colors) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).child(SharedString::from(label.to_owned())))
            .child(body)
            .when_some(error, |this, error| {
                this.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(error)))
            })
            .into_any_element()
    }

    /// The archived bots, with Restore, for the Team page.
    pub(crate) fn render_archived_bots(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let archived: Vec<Agent> = self.agents().iter().filter(|agent| agent.archived).cloned().collect();
        if archived.is_empty() {
            return None;
        }
        let mut rows = div().flex().flex_col().gap(px(4.0));
        for agent in &archived {
            let id = agent.id.clone();
            let provider = self.provider_of(agent);
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("archived-{}", agent.id)))
                    .h(px(36.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(theme::RADIUS_SM))
                    .hover(move |style| style.bg(colors.hover))
                    .child(Mark::for_agent(24.0, art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render())
                    .child(div().flex_1().truncate().text_size(px(theme::FS_SM)).child(SharedString::from(agent.name.clone())))
                    .child(self.ghost_button("Restore", &format!("restore-{}", agent.id), colors, cx, move |root, cx| {
                        root.restore_bot(id.clone(), cx)
                    })),
            );
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(theme::FS_XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.text_2)
                        .child("Archived bots"),
                )
                .child(rows)
                .into_any_element(),
        )
    }

    /// One chip in a row of choices.
    pub(crate) fn choice_chip(
        &self,
        label: &str,
        active: bool,
        id: &str,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("chip-{id}")))
            .h(px(24.0))
            .px(px(9.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.selected } else { colors.raised })
            .text_color(if active { colors.text_1 } else { colors.text_2 })
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.selected))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }
}

/// Keep `Window` in the signature list of this module honest.
#[allow(dead_code)]
fn _window(_: &mut Window) {}
