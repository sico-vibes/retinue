//! Guards for two bugs that only a screenshot or a crash could show, written as
//! tests over the source because the rule is about *where* code sits.
//!
//! 1. A text field's hook runs while that field is leased by the caller. GPUI
//!    takes a leased entity out of its map, so a hook that goes back through
//!    `entity.update`/`entity.read` on the same entity panics with
//!    "cannot update ... while it is already being updated", and the window
//!    aborts without a word. The hooks therefore receive the field itself.
//! 2. A menu drawn by taking it out of the window (`self.menu.take()`) is gone
//!    on the next repaint. This window repaints every frame (the marquee and the
//!    working shimmer), so the menu flickered for one frame and vanished.

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn read(name: &str) -> String {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(name))
            .unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    /// The body of each `input.set_on_*(..)` closure in `root.rs`, as lines.
    fn hook_bodies(root: &str) -> Vec<(String, Vec<String>)> {
        let mut found = Vec::new();
        let lines: Vec<&str> = root.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index];
            if line.contains(".set_on_change(") || line.contains(".set_on_submit(") || line.contains(".set_on_key(") {
                let kind = if line.contains(".set_on_change(") {
                    "change"
                } else if line.contains(".set_on_submit(") {
                    "submit"
                } else {
                    "key"
                };
                let mut body = Vec::new();
                index += 1;
                while index < lines.len() {
                    let current = lines[index].trim();
                    if current == "}));" {
                        break;
                    }
                    body.push(lines[index].to_string());
                    index += 1;
                }
                found.push((kind.to_owned(), body));
            }
            index += 1;
        }
        found
    }

    #[test]
    fn a_text_fields_hook_never_goes_back_through_the_field() {
        let root = read("ui/root.rs");
        let bodies = hook_bodies(&root);
        // The composer's submit, change and key hooks, the sidebar search, the quick
        // switcher and the rename field's submit.
        assert_eq!(bodies.len(), 6, "every text field's hooks: {bodies:?}");
        for (kind, body) in &bodies {
            for line in body {
                let code = line.split("//").next().unwrap_or("");
                for forbidden in [".composer.update", ".composer.read", ".quick_search.", ".sidebar_search.", ".rename_input."] {
                    assert!(
                        !code.contains(forbidden),
                        "the {kind} hook reaches its own field through `{forbidden}`:\n{line}\n\
                         A hook runs while that field is leased; edit the field it was handed instead."
                    );
                }
            }
        }
    }

    #[test]
    fn a_method_that_is_handed_the_box_never_touches_the_entity() {
        let composer = read("ui/composer.rs");
        let lines: Vec<&str> = composer.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            if lines[index].contains("&mut TextInput") {
                // Walk to the end of this function at its own indentation.
                let mut depth = 0i32;
                let mut started = false;
                let mut cursor = index;
                while cursor < lines.len() {
                    for ch in lines[cursor].chars() {
                        if ch == '{' {
                            depth += 1;
                            started = true;
                        }
                        if ch == '}' {
                            depth -= 1;
                        }
                    }
                    let code = lines[cursor].split("//").next().unwrap_or("");
                    assert!(
                        !code.contains("self.composer"),
                        "a method handed the box also reaches it through the entity:\n{}\n\
                         The box is leased by the caller; edit the one that was handed in.",
                        lines[cursor]
                    );
                    if started && depth <= 0 {
                        break;
                    }
                    cursor += 1;
                }
                index = cursor;
            }
            index += 1;
        }
    }

    #[test]
    fn a_user_bubbles_text_is_shaped_text_and_not_a_block() {
        // A bubble is as wide as its text, so a block inside it has no width to
        // wrap against: taffy gave it one character, and a long message became a
        // single column of letters down the whole pane (8 November 2026).
        let message = read("ui/message.rs");
        let bubble = message
            .split("Row::User { text, images, files, id, .. }")
            .nth(1)
            .expect("the user row");
        let body = &bubble[..bubble.find("Row::Assistant").unwrap_or(bubble.len())];
        assert!(body.contains("styled_inline(text.trim(), colors)"), "a one-line bubble draws the shaped text itself");
        // A text that wraps gets the bubble's explicit width: GPUI measures
        // wrapped text in a content-sized box at the wrong width (look pass, 8 Nov).
        assert!(body.contains("bubble.w(px(ctx.bubble_width()))"), "a wrapping user bubble has an explicit width");
        assert!(
            !body.contains("inline_text("),
            "a block inside a content-sized bubble collapses to one character wide; use `styled_inline`"
        );
        // The block form is still there for parents whose width is settled.
        assert!(message.contains("fn inline_text(") && message.contains("fn styled_inline("));
    }

    /// The body of one function: from its signature to the next `\n    fn`.
    fn body_of<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source.find(signature).unwrap_or_else(|| panic!("{signature} is missing"));
        let rest = &source[start..];
        let end = rest[1..].find("\n    fn ").map(|end| end + 1).unwrap_or(rest.len());
        &rest[..end]
    }

    /// Where a substring last appears, or -1.
    fn last_at(source: &str, needle: &str) -> isize {
        source.rfind(needle).map(|index| index as isize).unwrap_or(-1)
    }

    #[test]
    fn a_conversation_card_opens_on_the_press_of_the_whole_card() {
        // Two defects in one place (8 November 2026): the click sat on the block
        // that holds the title, so a press on the card's own padding — which is
        // drawn as the card and lights up on hover — did nothing; and a click
        // needs the same element under the pointer at press and at release, which
        // a re-sorting list cannot promise. The card answers the press now, over
        // its whole area.
        let conversations = read("ui/conversations.rs");
        let card = body_of(&conversations, "fn render_card(");
        let at = card.find("open_conversation").expect("the card opens its conversation");
        let before = &card[..at];
        assert!(
            last_at(before, "on_mouse_down") > last_at(before, "on_click"),
            "the card must open on the press of the card itself, not on a click of its title block"
        );
        assert!(
            card.matches("on_mouse_down").count() >= 2,
            "the card opens on the press, and its `…` keeps the press to itself"
        );
        // The open answer belongs to the row, next to the right-click, so the
        // padding around the title is part of it.
        let row_press = card
            .find("The press anywhere on the card opens its conversation")
            .expect("the card's own press handler");
        assert!(card[row_press..].contains("on_mouse_down"));
    }

    #[test]
    fn an_employee_row_and_its_cards_open_on_the_press() {
        let sidebar = read("ui/sidebar.rs");
        let employee = body_of(&sidebar, "fn render_employee(");
        let at = employee.find("open_conversation").expect("the employee row opens a conversation");
        assert!(
            last_at(&employee[..at], "on_mouse_down") > last_at(&employee[..at], "on_click"),
            "an employee row must select on the press: a click can be lost when the row moves"
        );

        // The conversations under an employee row, in the same tree.
        let start = employee
            .find("format!(\"conversation-{}\", session.id)")
            .expect("the tree's conversation cards");
        let tail = &employee[start..];
        let end = tail.find("collapsible(").unwrap_or(tail.len());
        let card = &tail[..end];
        assert!(
            card.contains("on_mouse_down") && card.contains("open_conversation"),
            "the tree's conversation cards open on the press too"
        );
        assert!(!card.contains("on_click"), "and not on a click that a moving row can lose");

        // The roster's `…` sits inside a row that selects, so its press must stop
        // before it reaches the row: otherwise it selects the bot as a side effect.
        let roster = body_of(&sidebar, "fn render_team_list(");
        let more = roster.find("roster-more-").expect("the roster's more button");
        let more_body = &roster[more..];
        let window = &more_body[..more_body.len().min(900)];
        assert!(
            window.contains("on_mouse_down"),
            "the roster's `…` must keep the press from selecting the bot under it"
        );
    }

    #[test]
    fn a_closed_menu_leaves_no_backdrop_catching_presses() {
        // The backdrop covers the window and closes the menu on any press. It must
        // exist only while a menu is open: if it outlived the menu it would eat the
        // next press, which is exactly a card or a row that does not answer.
        let menu = read("ui/menu.rs");
        let render = body_of(&menu, "pub(crate) fn render_menu_layer(");
        assert!(
            render.contains("let menu = self.menu.clone()?;"),
            "the layer must be empty (None) with no menu open"
        );
        assert_eq!(
            menu.matches("menu-backdrop").count(),
            1,
            "the backdrop is drawn in one place only"
        );
        assert!(render.contains("menu-backdrop"), "and that place is the menu layer");
        let close = body_of(&menu, "pub(crate) fn close_menu(");
        assert!(
            close.contains("self.menu.take()"),
            "closing a menu empties it, so the layer and its backdrop go away"
        );
    }

    #[test]
    fn the_open_menu_is_drawn_from_a_copy_so_a_repaint_keeps_it() {
        let menu = read("ui/menu.rs");
        let render = menu
            .split("pub(crate) fn render_menu_layer")
            .nth(1)
            .expect("the menu renderer");
        let body = &render[..render.find("\n    }").unwrap_or(render.len())];
        assert!(
            !body.contains("self.menu.take()"),
            "the menu is taken out of the window while drawing it, so the next repaint erases it; clone it"
        );
        assert!(body.contains("self.menu.clone()"), "the renderer draws from `self.menu.clone()`");
    }
}
