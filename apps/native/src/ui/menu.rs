//! The shared floating pieces: one menu at a time, a toast, and the rules that
//! close either (`ui/ComposerControls.tsx` `usePopoverDismiss`, `ui/TeamSidebar`
//! menus, `ui/MessageActions` menus).
//!
//! The pinned GPUI revision has `anchored()` and `deferred()`, so a menu is a
//! panel placed in window coordinates over everything else: it escapes the
//! scroll container that opened it, and it never changes the layout of the row
//! it hangs from. A transparent backdrop under the panel turns any click outside
//! into a close, which is the browser's `pointerdown` listener.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{anchored, deferred, div, prelude::*, px, AnyElement, Context, Corner, Pixels, Point, SharedString};

use crate::art::{self, Glyph};
use crate::theme::{self, Colors};
use crate::ui::root::Root;

/// What choosing a menu item does. Kept as a boxed closure so the window can
/// build a menu where it is rendered, with the arguments it already has.
pub type MenuAction = Rc<dyn Fn(&mut Root, &mut Context<Root>)>;

/// One thing a menu can hold. Cloneable because the menu is drawn from a copy:
/// taking it out of the window erased it on the next repaint, and something in
/// this window repaints every frame.
#[derive(Clone)]
pub enum MenuItem {
    Separator,
    Choice(MenuChoice),
}

#[derive(Clone)]
pub struct MenuChoice {
    /// Stable per menu, so the element id never moves between frames.
    pub key: String,
    pub label: String,
    /// A small line under the label: the menu explains what a choice does.
    pub hint: Option<String>,
    /// The mark at the right: `aria-checked="true"` in the React menus.
    pub checked: bool,
    /// "Marked in the bot's own menu": a tag such as "Bot default".
    pub tag: Option<String>,
    pub danger: bool,
    pub disabled: bool,
    pub icon: Option<Glyph>,
    pub action: MenuAction,
}

impl MenuChoice {
    pub fn new(key: impl Into<String>, label: impl Into<String>, action: MenuAction) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            hint: None,
            checked: false,
            tag: None,
            danger: false,
            disabled: false,
            icon: None,
            action,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        let hint = hint.into();
        self.hint = (!hint.is_empty()).then_some(hint);
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn tag(mut self, tag: Option<String>) -> Self {
        self.tag = tag;
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn icon(mut self, icon: Glyph) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// The open menu: what it says, where it hangs and what it offers.
#[derive(Clone)]
pub struct Menu {
    /// Which thing opened it, so a second click on the same trigger closes it.
    pub key: String,
    /// A heading row at the top ("Mode", "Attach").
    pub title: Option<String>,
    /// The pointer's position when it opened, in window coordinates.
    pub at: Point<Pixels>,
    pub width: f32,
    pub items: Vec<MenuItem>,
    /// One muted sentence at the bottom.
    pub note: Option<String>,
}

impl Menu {
    pub fn new(key: impl Into<String>, at: Point<Pixels>, items: Vec<MenuItem>) -> Self {
        Self { key: key.into(), title: None, at, width: 232.0, items, note: None }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        let note = note.into();
        self.note = (!note.is_empty()).then_some(note);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

/// Why a menu is being asked to close. Every one of these closes it, which is
/// the whole rule; the enum exists so the window can say which happened and the
/// tests can hold the rule still.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuClose {
    /// Escape, from anywhere in the window.
    Escape,
    /// A click that landed outside the panel.
    ClickedOutside,
    /// A choice was taken (and the state changed), so the menu is done.
    Chosen,
    /// The same trigger was pressed again.
    Toggled,
}

impl MenuClose {
    /// The rule, as one place: every one of these closes an open menu.
    pub fn closes(self) -> bool {
        // A menu that stays open after one of these would need a second press to
        // get rid of it, which no menu in the app does.
        matches!(self, MenuClose::Escape | MenuClose::ClickedOutside | MenuClose::Chosen | MenuClose::Toggled)
    }

    /// Whether focus should go back to the trigger, which is what a keyboard
    /// close does and a click elsewhere does not.
    pub fn restores_focus(self) -> bool {
        matches!(self, MenuClose::Escape | MenuClose::Chosen | MenuClose::Toggled)
    }
}

impl Root {
    /// Open a menu, or close the one already there when its trigger is pressed
    /// again.
    pub(crate) fn open_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        let same = self.menu.as_ref().map(|open| open.key == menu.key).unwrap_or(false);
        if same {
            self.close_menu(MenuClose::Toggled, cx);
            return;
        }
        self.menu = Some(menu);
        cx.notify();
    }

    /// Close the open menu for one of the four reasons, which all close it.
    pub(crate) fn close_menu(&mut self, reason: MenuClose, cx: &mut Context<Self>) {
        if reason.closes() && self.menu.take().is_some() {
            cx.notify();
        }
    }

    /// The pointer position of a click, in window coordinates, for a menu that
    /// hangs off it.
    pub(crate) fn click_position(&self, event: &gpui::ClickEvent, window: &mut gpui::Window) -> Point<Pixels> {
        let position = event.position();
        let _ = window;
        position
    }

    /// The open menu, as a panel over the window, with its backdrop. It is drawn
    /// from a **copy**: the window repaints while it is open (the marquee and the
    /// working shimmer run continuously), and a menu that was taken out of the
    /// window here disappeared on the next frame.
    pub(crate) fn render_menu_layer(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.clone()?;
        let backdrop = div()
            .id("menu-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                root.close_menu(MenuClose::ClickedOutside, cx);
            }))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|root, _: &gpui::MouseDownEvent, _window, cx| {
                root.close_menu(MenuClose::ClickedOutside, cx);
            }));

        let mut panel = div()
            .id("menu-panel")
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _: &gpui::MouseDownEvent, _window, cx| {
                // The press belongs to the menu: it must not reach the backdrop
                // under it, which would close the menu before the item answers.
                cx.stop_propagation();
            }))
            .w(px(menu.width))
            .p(px(3.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong);
        if let Some(title) = menu.title.clone() {
            panel = panel.child(
                div()
                    .px(px(8.0))
                    .pt(px(5.0))
                    .pb(px(3.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(title)),
            );
        }
        let mut seen = 0usize;
        for item in menu.items {
            match item {
                MenuItem::Separator => {
                    panel = panel.child(div().h(px(1.0)).mx(px(4.0)).my(px(3.0)).bg(colors.line));
                }
                MenuItem::Choice(choice) => {
                    seen += 1;
                    panel = panel.child(menu_choice_row(choice, seen, &menu.key, colors, cx));
                }
            }
        }
        if let Some(note) = menu.note.clone() {
            panel = panel.child(
                div()
                    .px(px(8.0))
                    .pt(px(4.0))
                    .pb(px(2.0))
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_3)
                    .child(SharedString::from(note)),
            );
        }
        let anchored_panel = anchored()
            .position(menu.at)
            .anchor(Corner::TopLeft)
            .snap_to_window_with_margin(px(8.0))
            .child(panel);
        Some(div().absolute().top_0().left_0().size_full().child(backdrop).child(deferred(anchored_panel).with_priority(20)).into_any_element())
    }
}

fn menu_choice_row(choice: MenuChoice, index: usize, menu_key: &str, colors: Colors, cx: &mut Context<Root>) -> AnyElement {
    let disabled = choice.disabled;
    let danger = choice.danger;
    let label_color = if disabled {
        colors.text_3
    } else if danger {
        colors.status_danger
    } else {
        colors.text_1
    };
    let mut row = div()
        .id(SharedString::from(format!("menu-{menu_key}-{}-{index}", choice.key)))
        .h(px(if choice.hint.is_some() { 38.0 } else { 28.0 }))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(theme::RADIUS_SM))
        .text_size(px(theme::FS_XS))
        .text_color(label_color)
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .active(move |style| style.bg(colors.selected_strong))
        })
        .when(disabled, |this| this.opacity(0.55));
    if let Some(icon) = choice.icon {
        row = row.child(art::glyph(icon, 12.0, if disabled { colors.text_3 } else { colors.text_2 }));
    }
    let mut copy = div().flex_1().min_w(px(0.0)).flex().flex_col();
    let mut line = div().flex().items_center().gap(px(6.0)).child(SharedString::from(choice.label.clone()));
    if let Some(tag) = choice.tag.clone() {
        line = line.child(
            div()
                .px(px(5.0))
                .rounded(px(4.0))
                .bg(colors.hover)
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child(SharedString::from(tag)),
        );
    }
    copy = copy.child(line);
    if let Some(hint) = choice.hint.clone() {
        copy = copy.child(
            div()
                .truncate()
                .text_size(px(theme::FS_2XS))
                .text_color(colors.text_3)
                .child(SharedString::from(hint)),
        );
    }
    row = row.child(copy);
    if choice.checked {
        row = row.child(art::glyph(Glyph::CHECK, 12.0, colors.text_1));
    }
    if !disabled {
        let action = choice.action.clone();
        row = row.on_click(cx.listener(move |root, _: &gpui::ClickEvent, _window, cx| {
            root.close_menu(MenuClose::Chosen, cx);
            action(root, cx);
        }));
    }
    row.into_any_element()
}

/// A short notice in the window's bottom-left corner: "Copied", "Renamed", or
/// what went wrong. It leaves after four seconds.
pub struct Toast {
    pub text: String,
    pub until: Instant,
}

/// How long a toast stays, in milliseconds.
pub const TOAST_MS: u64 = 4_000;

impl Root {
    /// Show a toast, and ask for one repaint when it is due to leave.
    pub(crate) fn toast(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(Toast { text: text.into(), until: Instant::now() + Duration::from_millis(TOAST_MS) });
        cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(TOAST_MS + 40)).await;
            cx.update(|app| {
                if let Some(root) = this.upgrade() {
                    root.update(app, |root, cx| {
                        let live = root.toast.as_ref().map(|toast| toast.until > Instant::now()).unwrap_or(false);
                        if !live {
                            root.toast = None;
                            cx.notify();
                        }
                    });
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn render_toast(&mut self, colors: Colors) -> Option<AnyElement> {
        let toast = self.toast.take()?;
        let live = toast.until > Instant::now();
        if live {
            self.toast = Some(toast);
        }
        let toast = self.toast.as_ref()?;
        let panel = div()
            .mx(px(12.0))
            .mb(px(12.0))
            .px(px(12.0))
            .py(px(7.0))
            .max_w(px(420.0))
            .truncate()
            .rounded(px(theme::RADIUS_MD))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong)
            .text_size(px(theme::FS_XS))
            .text_color(colors.text_1)
            .child(SharedString::from(toast.text.clone()));
        Some(
            div()
                .absolute()
                .left_0()
                .bottom_0()
                .flex()
                .flex_col()
                .items_start()
                .child(panel)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_of_leaving_a_menu_closes_it() {
        // The window closes a menu on the trigger's own press, which is the same
        // rule as a click outside.
        assert!(MenuClose::Toggled.closes() && MenuClose::ClickedOutside.closes());
        for way in [MenuClose::Escape, MenuClose::ClickedOutside, MenuClose::Chosen, MenuClose::Toggled] {
            assert!(way.closes(), "{way:?} closes the menu");
        }
        // A click elsewhere leaves focus where the owner put it; the others go
        // back to the trigger.
        assert!(!MenuClose::ClickedOutside.restores_focus());
        assert!(MenuClose::Escape.restores_focus());
        assert!(MenuClose::Chosen.restores_focus());
    }

    #[test]
    fn a_toast_is_due_after_four_seconds() {
        assert_eq!(TOAST_MS, 4_000);
    }
}
