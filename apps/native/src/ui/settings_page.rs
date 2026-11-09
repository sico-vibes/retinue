//! Settings, first pass: Appearance (theme and density), Data (where the native
//! database is, and copying the installed Retinue data again) and About.
//!
//! Everything here is the window's own state; the daemon keeps product settings.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, SharedString};

use crate::art::{self, Glyph};
use crate::daemon::DaemonState;
use crate::paths;
use crate::theme::{self, Colors, Density, ThemeChoice};
use crate::ui::root::Root;

impl Root {
    pub(crate) fn render_settings_page(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let header = div()
            .h(px(46.0))
            .flex_none()
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(colors.line)
            .child(
                div()
                    .id("settings-back")
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(theme::RADIUS_SM))
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.hover))
                    .child(art::glyph(Glyph::ARROW_LEFT, 15.0, colors.text_2))
                    .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| root.back_to_conversation(cx))),
            )
            .child(div().text_size(px(theme::FS_LG)).font_weight(FontWeight::SEMIBOLD).child("Settings"));

        let appearance = {
            let mut themes = div().flex().gap(px(2.0));
            for choice in ThemeChoice::ALL {
                themes = themes.child(self.choice_button(
                    choice.label(),
                    self.theme_choice == choice,
                    format!("theme-{}", choice.key()),
                    colors,
                    cx,
                    move |root, cx| root.set_theme(choice, cx),
                ));
            }
            let mut densities = div().flex().gap(px(2.0));
            for choice in Density::ALL {
                densities = densities.child(self.choice_button(
                    choice.label(),
                    self.density == choice,
                    format!("density-{}", choice.key()),
                    colors,
                    cx,
                    move |root, cx| root.set_density(choice, cx),
                ));
            }
            section(
                "Appearance",
                colors,
                vec![
                    field("Theme", "System follows Windows; Light and Dark pin it.", themes.into_any_element(), colors),
                    field("Density", "Row heights and gaps, everywhere a list is.", densities.into_any_element(), colors),
                    field(
                        "Plan usage in the sidebar",
                        "One line at the foot of the sidebar with the limit closest to running out. Reading a coding agent's own sign-in is off until you turn it on.",
                        {
                            let on = self.usage_bar;
                            let mut row = div().flex().gap(px(2.0));
                            row = row.child(self.choice_button("On", on, "usage-on".to_owned(), colors, cx, |root, cx| {
                                root.set_usage_bar(true, cx);
                                root.load_quotas(cx);
                            }));
                            row = row.child(self.choice_button("Off", !on, "usage-off".to_owned(), colors, cx, |root, cx| {
                                root.set_usage_bar(false, cx)
                            }));
                            row.into_any_element()
                        },
                        colors,
                    ),
                ],
            )
        };

        let data = {            let path = self.db_path.display().to_string();
            let installed = paths::installed_db_path();
            let installed_line = if installed.is_file() {
                format!("The installed Retinue database is at {}.", installed.display())
            } else {
                "There is no installed Retinue database on this computer.".to_owned()
            };
            let busy = if self.data_busy { "Copying…" } else { "Copy my Retinue data again" };
            let button = div()
                .id("copy-data-again")
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .rounded(px(theme::RADIUS_SM))
                .bg(colors.raised)
                .text_color(colors.text_1)
                .text_size(px(theme::FS_XS))
                .when(self.data_busy, |this| this.opacity(0.55))
                .when(!self.data_busy, |this| this.cursor_pointer().hover(move |style| style.bg(colors.selected)))
                .child(SharedString::from(busy.to_owned()))
                .on_click(cx.listener(|root, _: &gpui::ClickEvent, _window, cx| {
                    if !root.data_busy {
                        root.ask_copy_data_again(cx);
                    }
                }));
            section(
                "Data",
                colors,
                vec![
                    field(
                        "This preview's database",
                        "It never shares a database with the installed Retinue.",
                        div()
                            .text_size(px(theme::FS_XS))
                            .font_family(theme::MONO_FONT)
                            .text_color(colors.text_2)
                            .child(SharedString::from(path))
                            .into_any_element(),
                        colors,
                    ),
                    field(
                        "Copy the installed data",
                        &installed_line,
                        button.into_any_element(),
                        colors,
                    ),
                ],
            )
        };

        let about = {
            let daemon = match (&self.daemon_state, self.daemon_version.clone()) {
                (DaemonState::Running, Some(version)) => format!("The daemon is running, version {version}."),
                (DaemonState::Running, None) => "The daemon is running.".to_owned(),
                (DaemonState::Restarting { attempt, .. }, _) => format!("The daemon stopped; restart attempt {attempt}."),
                (DaemonState::Stopped(_), _) => "The daemon is not running.".to_owned(),
                (DaemonState::Missing(message), _) => message.clone(),
                (DaemonState::Starting, _) => "The daemon is starting.".to_owned(),
            };
            section(
                "About",
                colors,
                vec![
                    field(
                        "Retinue (Rust preview)",
                        "The native port of the desktop app.",
                        div()
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(format!("Version {}", theme::VERSION)))
                            .into_any_element(),
                        colors,
                    ),
                    field(
                        "Daemon",
                        "The process that owns sessions, turns and storage.",
                        div()
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(daemon))
                            .into_any_element(),
                        colors,
                    ),
                ],
            )
        };

        let column = div()
            .max_w(px(720.0))
            .mx_auto()
            .flex()
            .flex_col()
            .gap(px(18.0))
            .px(px(24.0))
            .py(px(20.0))
            .child(appearance)
            .child(data)
            .child(about);

        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(div().id("settings-scroll").flex_1().w_full().overflow_y_scroll().child(column))
            .into_any_element()
    }

    fn choice_button(
        &self,
        label: &str,
        active: bool,
        id: String,
        colors: Colors,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Root, &mut Context<Root>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("choice-{id}")))
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(theme::RADIUS_SM))
            .bg(if active { colors.selected } else { gpui::rgba(0x00000000) })
            .text_color(if active { colors.text_1 } else { colors.text_2 })
            .text_size(px(theme::FS_XS))
            .cursor_pointer()
            .hover(move |style| style.text_color(colors.text_1))
            .child(SharedString::from(label.to_owned()))
            .on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| action(root, cx)))
            .into_any_element()
    }
}

fn section(title: &str, colors: Colors, fields: Vec<AnyElement>) -> AnyElement {
    let mut body = div().flex().flex_col().gap(px(14.0));
    for (index, item) in fields.into_iter().enumerate() {
        if index > 0 {
            body = body.child(div().h(px(1.0)).w_full().bg(colors.line));
        }
        body = body.child(item);
    }
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(theme::FS_XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.text_2)
                .child(SharedString::from(title.to_owned())),
        )
        .child(
            div()
                .p(px(14.0))
                .rounded(px(theme::RADIUS_MD))
                .border_1()
                .border_color(colors.line)
                .child(body),
        )
        .into_any_element()
}

fn field(title: &str, hint: &str, control: AnyElement, colors: Colors) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(px(16.0))
        .child(
            div()
                .w(px(220.0))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(div().text_size(px(theme::FS_SM)).child(SharedString::from(title.to_owned())))
                .child(
                    div()
                        .text_size(px(theme::FS_XS))
                        .text_color(colors.text_3)
                        .child(SharedString::from(hint.to_owned())),
                ),
        )
        .child(div().flex_1().min_w(px(0.0)).flex().flex_col().items_start().gap(px(6.0)).child(control))
        .into_any_element()
}
