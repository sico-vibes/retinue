//! The companion window's view: the compact bar, the welcome, and the expanded
//! split layout of `CompanionTeam.tsx` / `CompanionAttention.tsx` /
//! `CompanionConversationPicker.tsx` in Haze.
//!
//! Haze's companion surfaces are an **opaque smoke** palette
//! (`ui/themes/haze.css` on `.companion-surface`): the window is transparent, so
//! its rounded corners cut out, and the capsule paints `#202124` (dark) or
//! `#f4f2ee` (light) — no blur, exactly as the React app documents. The washes
//! (`--companion-wash-*` radial gradients) are a flat tint here, because GPUI at
//! this revision has no gradient paint; the same amber marks what needs you.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gpui::{
    div, point, prelude::*, px, rgba, rgb, AnyElement, AnyWindowHandle, App, Bounds, Context,
    Entity, ExternalPaths, MouseButton, Pixels, SharedString, Size, Subscription, Window,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};

use crate::art::{self, Mark};
use crate::companion::attention::{self, Approval, BlobState};
use crate::companion::drop::{self, DropPhase};
use crate::companion::fsm::{CompanionFsm, Mode};
use crate::companion::layout::{self, Preset};
use crate::companion::peek::{self, Peek, PeekSession};
use crate::companion::positions::{self, WorkArea};
use crate::companion::settings::{CompanionSettings, SavedSize};
use crate::companion::shared::Shared;
use crate::companion::status::{self, Status};
use crate::companion::window as floating;
use crate::dictation::copy::IslandPhase;
use crate::dictation::engine::Owner as DictationOwner;
use crate::dictation::Dictation;
use crate::settings_file;
use crate::sound::{CompanionCue, SoundEngine};
use crate::state::{Agent, Session};
use crate::theme::{self, Colors};
use crate::trace;
use crate::ui::text_input::TextInput;

/// How often the window checks its own deadlines.
const TICK: Duration = Duration::from_millis(150);
/// How many teammates the compact bar shows (Haze keeps three).
const COMPACT_PEERS: usize = 3;
/// The greeting's own length (`GREETING_END_MS`).
const GREETING_MS: u64 = 4_600;

/// A move in progress: where the cursor was and where the window was.
struct MoveDrag {
    cursor: (i32, i32),
    rect: positions::Rect,
}

/// A resize in progress, measured from the same point every step.
struct ResizeDrag {
    edge: positions::ResizeEdge,
    cursor: (i32, i32),
    rect: positions::Rect,
    min: (i32, i32),
    max: (i32, i32),
}

/// A size change being animated: the rectangle it had and the one it wants.
struct Transition {
    from: positions::Rect,
    to: positions::Rect,
    expand: bool,
    started: Instant,
}

pub struct Companion {
    shared: Entity<Shared>,
    dictation: Entity<Dictation>,
    sound: std::sync::Arc<SoundEngine>,
    pub settings: CompanionSettings,
    fsm: CompanionFsm,
    composer: Entity<TextInput>,
    window: Option<AnyWindowHandle>,
    hwnd: Option<isize>,
    system_is_light: bool,
    /// The states the last peek looked at.
    previous_sessions: Vec<PeekSession>,
    peek: Option<Peek>,
    peek_names: Vec<String>,
    drop: DropPhase,
    drop_seen_at: Option<Instant>,
    /// Approvals set aside with "Later".
    held: BTreeMap<String, Instant>,
    queue_index: usize,
    drag: Option<MoveDrag>,
    resize: Option<ResizeDrag>,
    picker_open: bool,
    transition: Option<Transition>,
    /// A mode change waiting for the ticker: Win32 must not be called from
    /// inside a paint, which re-enters GPUI's borrowed state.
    apply_requested: Option<bool>,
    greeting_started: Instant,
    last_revision: u64,
    /// Kept alive: the shared state's changes notify this view.
    _subscription: Subscription,
}

impl Companion {
    pub fn new(
        shared: Entity<Shared>,
        dictation: Entity<Dictation>,
        sound: std::sync::Arc<SoundEngine>,
        settings: CompanionSettings,
        system_is_light: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let this = cx.weak_entity();
        let composer = cx.new(|cx| {
            let mut input = TextInput::new("Message your bot…", px(18.0), px(theme::FS_SM), cx);
            let this = this.clone();
            // The hook is handed the box itself: reaching it through its entity
            // here would update it while it is already being updated (a panic).
            input.set_on_submit(Box::new(move |input, _window, cx: &mut App| {
                let text = input.text().trim().to_owned();
                if text.is_empty() {
                    return;
                }
                if let Some(companion) = this.upgrade() {
                    companion.update(cx, |companion, cx| companion.send_text(text, cx));
                    input.clear_quiet();
                }
            }));
            input
        });
        let _subscription = cx.observe(&shared, |this, _shared, cx| this.on_shared_update(cx));

        // The clock: the greeting, the deadlines, the peek and the drag grace.
        let this = cx.weak_entity();
        cx.spawn(async move |_, cx: &mut gpui::AsyncApp| loop {
            cx.background_executor().timer(TICK).await;
            if this.upgrade().is_none() {
                break;
            }
            cx.update(|app| {
                if let Some(companion) = this.upgrade() {
                    companion.update(app, |companion, cx| companion.tick(cx));
                }
            });
        })
        .detach();

        let mut fsm = CompanionFsm::new();
        if settings.visible {
            if settings.greet_on_launch {
                fsm.launch(Instant::now());
            } else {
                fsm.reveal(Instant::now());
            }
        }
        // The greeting animates at the frame rate, not at the clock's tick.
        if fsm.mode() == Mode::Welcome {
            let this = cx.weak_entity();
            cx.spawn(async move |_, cx: &mut gpui::AsyncApp| {
                let began = Instant::now();
                while began.elapsed() < Duration::from_millis(GREETING_MS) {
                    cx.background_executor().timer(Duration::from_millis(16)).await;
                    if this.upgrade().is_none() {
                        break;
                    }
                    cx.update(|app| {
                        if let Some(companion) = this.upgrade() {
                            companion.update(app, |_, cx| cx.notify());
                        }
                    });
                }
            })
            .detach();
        }

        Self {
            shared,
            dictation,
            sound,
            settings,
            fsm,
            composer,
            window: None,
            hwnd: None,
            system_is_light,
            previous_sessions: Vec::new(),
            peek: None,
            peek_names: Vec::new(),
            drop: DropPhase::Idle,
            drop_seen_at: None,
            held: BTreeMap::new(),
            queue_index: 0,
            drag: None,
            resize: None,
            picker_open: false,
            transition: None,
            apply_requested: None,
            greeting_started: Instant::now(),
            last_revision: 0,
            _subscription,
        }
    }

    fn colors(&self) -> Colors {
        companion_colors(!self.system_is_light)
    }

    pub fn mode(&self) -> Mode {
        self.fsm.mode()
    }

    pub fn handle(&self) -> Option<AnyWindowHandle> {
        self.window
    }

    /// Bring the main window forward, for the buttons that say "Open in
    /// Bitbots" but have no conversation to open.
    fn activate_main(&self, cx: &mut Context<Self>) {
        let handle = self.shared.read(cx).main_window;
        if let Some(handle) = handle {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
    }

    pub fn hwnd(&self) -> Option<isize> {
        self.hwnd
    }

    pub fn composer(&self) -> &Entity<TextInput> {
        &self.composer
    }

    // ---- the shared state -----------------------------------------------------

    fn on_shared_update(&mut self, cx: &mut Context<Self>) {
        let revision = self.shared.read(cx).revision;
        if revision == self.last_revision {
            return;
        }
        self.last_revision = revision;
        self.track_peek(cx);
        self.track_attention(cx);
        cx.notify();
    }

    /// A bot that was busy and is now quiet earns one short banner (`C7`).
    fn track_peek(&mut self, cx: &mut Context<Self>) {
        let (next, names, focused) = {
            let shared = self.shared.read(cx);
            let Some(snapshot) = shared.snapshot() else { return };
            let rows: Vec<PeekSession> = snapshot
                .sessions
                .iter()
                .filter(|session| !session.archived && !session.is_side())
                .map(|session| PeekSession::new(session.id.clone(), session.state.as_deref()))
                .collect();
            let names: BTreeMap<String, String> = snapshot.agents.iter().map(|agent| (agent.id.clone(), agent.name.clone())).collect();
            let focused = shared.session_id().map(str::to_owned);
            (rows, names, focused)
        };
        let finished = peek::detect_finished(&self.previous_sessions, &next, focused.as_deref());
        self.previous_sessions = next;
        if finished.is_empty() {
            return;
        }
        let now = millis();
        if let Some(peek) = peek::next_peek(self.peek.clone(), &finished, now, peek::PEEK_WINDOW_MS) {
            self.peek_names = peek.ids.iter().map(|id| names.get(id).cloned().unwrap_or_else(|| "A bot".to_owned())).collect();
            if !self.settings.focus_mode {
                self.sound.play(CompanionCue::Peek);
                trace::log(format!("companion: {} finished a turn", finished.len()));
            }
            self.peek = Some(peek);
        }
        if self.fsm.mode() == Mode::Hidden {
            self.fsm.reveal(Instant::now());
            self.request_mode(false);
        }
    }

    /// An approval opens the companion with the request in front; Focus mode
    /// still shows it (only the peek and its cue are suppressed).
    fn track_attention(&mut self, cx: &mut Context<Self>) {
        let waiting = {
            let shared = self.shared.read(cx);
            let Some(snapshot) = shared.snapshot() else { return };
            !attention::approvals(snapshot, &self.held, Instant::now()).is_empty()
        };
        if waiting && self.fsm.mode() != Mode::Home && !self.fsm.pinned {
            self.fsm.force_home(Instant::now());
            self.request_mode(false);
            if !self.settings.focus_mode {
                self.sound.play(CompanionCue::Approval);
            }
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        if self.fsm.poll(now) {
            self.request_mode(true);
        }
        if let Some(animate) = self.apply_requested.take() {
            self.apply_mode(animate, cx);
        }
        if self.fsm.mode() == Mode::Welcome && now.duration_since(self.greeting_started) > Duration::from_millis(GREETING_MS) {
            self.fsm.greet_complete(now);
        }
        // A file drag that has not reported itself for the grace period has
        // left the island (`DROP_LEAVE_GRACE_MS`).
        if self.drop == DropPhase::Over {
            if let Some(seen) = self.drop_seen_at {
                let elapsed = now.duration_since(seen).as_millis() as u64;
                if drop::grace_over(0, elapsed) {
                    self.drop = drop::advance_drop_zone(self.drop, drop::DropEvent::Leave);
                    cx.notify();
                }
            }
        }
        if self.peek.as_ref().map(|peek| !peek::peek_visible(peek, millis())).unwrap_or(false) {
            self.peek = None;
            cx.notify();
        }
        self.held.retain(|_, until| *until > now);
        if self.fsm.mode() == Mode::Welcome || self.dictation.read(cx).phase.active() {
            cx.notify();
        }
    }

    // ---- the window -----------------------------------------------------------

    /// Record the window GPUI opened for this view, once.
    fn adopt_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.hwnd.is_some() {
            return;
        }
        let Some(hwnd) = floating::hwnd_of(window) else { return };
        self.hwnd = Some(hwnd);
        self.window = Some(window.window_handle());
        let this = cx.weak_entity();
        let handle = window.window_handle();
        self.shared.update(cx, |shared, _| shared.companion_window = Some(handle));
        self.dictation.update(cx, |dictation, _| dictation.attach_companion(this, hwnd));
        // The host provider never activates the companion: the owner is typing
        // somewhere else and the island must not steal the caret.
        self.apply_requested = Some(false);
    }

    /// Ask for the mode's size to be applied on the next tick, and never from
    /// inside a paint: ShowWindow and SetWindowPos send window messages that
    /// re-enter GPUI while its state is borrowed.
    fn request_mode(&mut self, animate: bool) {
        self.apply_requested = Some(self.apply_requested.unwrap_or(false) || animate);
    }

    /// Apply the current mode's size, anchored on the bottom centre, with the
    /// React app's own grow and shrink curves.
    fn apply_mode(&mut self, animate: bool, cx: &mut Context<Self>) {
        let mode = self.fsm.mode();
        let Some(hwnd) = self.hwnd else { return };
        if mode == Mode::Hidden {
            floating::hide(cx.background_executor(), hwnd);
            self.save_position(hwnd);
            cx.notify();
            return;
        }
        floating::show(cx.background_executor(), hwnd);
        let Some(rect) = floating::window_rect(hwnd) else { cx.notify(); return };
        let scale = floating::dpi_scale(hwnd);
        let areas = floating::work_areas();
        let area = floating::area_for_window(hwnd)
            .or_else(|| areas.first().cloned())
            .unwrap_or(WorkArea { x: 0, y: 0, width: 1920, height: 1080, name: String::new() });
        let (width, height) = layout::size_for(mode, self.settings.saved_size(), area.logical(scale));
        let size = ((width * scale).round() as i32, (height * scale).round() as i32);
        // A remembered spot wins over the window's current rectangle: the outer
        // rect carries GPUI's own border, so re-deriving the spot from it would
        // shift the island a few pixels up on every launch.
        let remembered = self.settings.positions.get(&area.name).map(|point| {
            let centre = area.x + (point.anchor_x.unwrap_or(0.5).clamp(0.0, 1.0) * area.width as f64).round() as i32;
            (centre, point.y)
        });
        let (centre, bottom) = remembered.unwrap_or((rect.0 + rect.2 / 2, rect.1 + rect.3));
        let target = positions::clamp_into(&areas, (centre - size.0 / 2, bottom - size.1, size.0, size.1), (centre, bottom));
        if animate && crate::ui::motion::animations_enabled() && (rect.2, rect.3) != (size.0, size.1) {
            self.transition = Some(Transition {
                from: rect,
                to: target,
                expand: (size.0 * size.1) > (rect.2 * rect.3),
                started: Instant::now(),
            });
            self.spawn_transition(cx);
        } else {
            floating::place(cx.background_executor(), hwnd, target.0, target.1, target.2, target.3);
            self.transition = None;
            self.save_position(hwnd);
        }
        cx.notify();
    }

    fn spawn_transition(&mut self, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        cx.spawn(async move |_, cx: &mut gpui::AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(16)).await;
            let Some(companion) = this.upgrade() else { break };
            let keep_going = companion.update(cx, |companion, cx| companion.step_transition(cx));
            if !keep_going {
                break;
            }
        })
        .detach();
    }

    /// One frame of a grow or a shrink. False once the transition is done.
    fn step_transition(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(transition) = self.transition.as_ref() else { return false };
        let Some(hwnd) = self.hwnd else { return false };
        let elapsed = transition.started.elapsed().as_secs_f64();
        let duration = layout::transition_seconds(transition.expand);
        let progress = layout::transition_progress(transition.expand, elapsed);
        let (from, to) = (transition.from, transition.to);
        let lerp = |a: i32, b: i32| (a as f64 + (b - a) as f64 * progress).round() as i32;
        let width = lerp(from.2, to.2);
        let height = lerp(from.3, to.3);
        let centre = from.0 + from.2 / 2;
        let bottom = from.1 + from.3;
        let placed = positions::clamp_into(&floating::work_areas(), (centre - width / 2, bottom - height, width, height), (centre, bottom));
        floating::place(cx.background_executor(), hwnd, placed.0, placed.1, placed.2, placed.3);
        if elapsed >= duration {
            floating::place(cx.background_executor(), hwnd, to.0, to.1, to.2, to.3);
            self.transition = None;
            self.save_position(hwnd);
            cx.notify();
            return false;
        }
        cx.notify();
        true
    }

    /// Remember where the window is, per display (`companionPositions.ts`).
    fn save_position(&mut self, hwnd: isize) {
        let Some(rect) = floating::window_rect(hwnd) else { return };
        let Some(area) = floating::area_for_window(hwnd) else { return };
        let scale = floating::dpi_scale(hwnd) as f64;
        let point = positions::stored_point(rect, &area, scale);
        self.settings.positions.remember(&area.name, point);
        if self.fsm.mode() == Mode::Home && !self.settings.maximised {
            self.settings.size = Some(SavedSize::new(rect.2 as f32 / scale as f32, rect.3 as f32 / scale as f32));
        }
        self.settings.visible = self.fsm.mode().visible();
        if let Err(error) = settings_file::write_companion(&crate::paths::settings_file_path(), &self.settings) {
            trace::log(format!("companion: could not save its position: {error}"));
        }
    }

    // ---- actions --------------------------------------------------------------

    /// Ctrl+Alt+B, and the collapse chevron.
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.fsm.toggle(Instant::now());
        self.after_mode_change(cx);
    }

    pub fn expand(&mut self, cx: &mut Context<Self>) {
        self.fsm.click(Instant::now());
        self.after_mode_change(cx);
    }

    pub fn collapse(&mut self, cx: &mut Context<Self>) {
        self.fsm.force_petit(true, Instant::now());
        self.after_mode_change(cx);
    }

    /// Show the companion from the outside (the sidebar's switch, in slice 6).
    pub fn reveal(&mut self, cx: &mut Context<Self>) {
        self.fsm.reveal(Instant::now());
        self.after_mode_change(cx);
    }

    /// The pointer came on or left the island. Entering cancels a pending
    /// collapse; leaving the expanded companion schedules one
    /// (`IslandStateMachine::mouseEntered` / `mouseLeft`).
    fn hover_changed(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let before = self.fsm.mode();
        if hovered {
            self.fsm.mouse_entered(Instant::now());
        } else {
            self.fsm.mouse_left(Instant::now());
        }
        if self.fsm.mode() != before {
            self.after_mode_change(cx);
        } else {
            cx.notify();
        }
    }

    /// "Large" / "Normal": the size preset (`set_companion_size_preset`).
    fn size_button(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let large = self.settings.maximised;
        let next = if large { Preset::Normal } else { Preset::Large };
        div()
            .id("companion-size")
            .flex_none()
            .h(px(26.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.apply_preset(next, cx)))
            .child(
                div()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_2)
                    .child(SharedString::from(if large { "Normal" } else { "Large" })),
            )
            .into_any_element()
    }

    fn after_mode_change(&mut self, _cx: &mut Context<Self>) {
        let mode = self.fsm.mode();
        match mode {
            Mode::Home => {
                self.sound.play(CompanionCue::Open);
                self.drop = DropPhase::Idle;
            }
            Mode::Petit => {
                self.sound.play(CompanionCue::Close);
                self.picker_open = false;
            }
            _ => {}
        }
        self.request_mode(true);
    }

    /// A teammate: that bot's conversation opens in both windows.
    pub fn focus_agent(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.sound.play(CompanionCue::Blip);
        let session = {
            let shared = self.shared.read(cx);
            shared.snapshot().and_then(|snapshot| snapshot.conversations_for(&agent_id).first().map(|session| session.id.clone()))
        };
        if let Some(session_id) = session {
            self.shared.update(cx, |shared, cx| shared.open_in_main(Some(agent_id), session_id, cx));
        } else {
            self.activate_main(cx);
        }
        self.picker_open = false;
        cx.notify();
    }

    /// The picker: choose one of the focused bot's conversations.
    pub fn pick_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        let agent_id = self.shared.read(cx).session(&session_id).and_then(|session| session.agent_id.clone());
        self.shared.update(cx, |shared, cx| shared.open_in_main(agent_id, session_id, cx));
        self.picker_open = false;
        self.sound.play(CompanionCue::Open);
        cx.notify();
    }

    /// Send the composer's text to the picked conversation.
    /// The Send button: read and clear the box from outside its own update.
    pub fn send(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.composer.update(cx, |input, _cx| input.clear_quiet());
        self.send_text(text, cx);
    }

    fn send_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.shared.update(cx, |shared, cx| shared.send_prompt(text, cx));
        self.sound.play(CompanionCue::Send);
        cx.notify();
    }

    /// Allow, through the same `permission.reply` the main window sends.
    pub fn allow(&mut self, approval: &Approval, cx: &mut Context<Self>) {
        self.shared.update(cx, |shared, cx| {
            shared.send(crate::daemon::Command::ReplyPermission {
                permission_id: approval.id.clone(),
                choice: approval.allow.clone(),
                answers: None,
                remember: false,
                remember_project: false,
            }, cx);
        });
        self.sound.play(CompanionCue::Approve);
        cx.notify();
    }

    pub fn deny(&mut self, approval: &Approval, cx: &mut Context<Self>) {
        self.shared.update(cx, |shared, cx| {
            shared.send(crate::daemon::Command::ReplyPermission {
                permission_id: approval.id.clone(),
                choice: approval.deny.clone(),
                answers: None,
                remember: false,
                remember_project: false,
            }, cx);
        });
        cx.notify();
    }

    /// "Later": set this request aside for five minutes.
    pub fn later(&mut self, approval: &Approval, cx: &mut Context<Self>) {
        attention::hold(&mut self.held, &approval.id, Instant::now());
        self.queue_index = 0;
        cx.notify();
    }

    /// "Open in Bitbots": the main window comes forward on this conversation.
    pub fn open_in_bitbots(&mut self, cx: &mut Context<Self>) {
        let (session_id, agent) = {
            let shared = self.shared.read(cx);
            let session_id = shared.session_id().map(str::to_owned);
            let agent = session_id.as_deref().and_then(|id| shared.session(id)).and_then(|session| session.agent_id.clone());
            (session_id, agent)
        };
        match session_id {
            Some(session_id) => self.shared.update(cx, |shared, cx| shared.open_in_main(agent, session_id, cx)),
            None => self.activate_main(cx),
        }
        cx.notify();
    }

    /// A dictation transcript, appended the way the React app's composer does.
    pub fn append_dictated(&mut self, text: &str, cx: &mut Context<Self>) {
        let composer = self.composer.clone();
        crate::dictation::append_text(&composer, text, cx);
        cx.notify();
    }

    // ---- the mouse ------------------------------------------------------------

    fn begin_move(&mut self, cx: &mut Context<Self>) {
        let Some(hwnd) = self.hwnd else { return };
        let (Some(cursor), Some(rect)) = (floating::cursor(), floating::window_rect(hwnd)) else { return };
        self.drag = Some(MoveDrag { cursor, rect });
        cx.notify();
    }

    fn move_window(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.as_ref() else { return };
        let Some(hwnd) = self.hwnd else { return };
        let Some(cursor) = floating::cursor() else { return };
        let moved = positions::moved_rect(drag.rect, (cursor.0 - drag.cursor.0, cursor.1 - drag.cursor.1));
        // A dragged island may hang over an edge; it is pulled back on release.
        floating::place(cx.background_executor(), hwnd, moved.0, moved.1, moved.2, moved.3);
        cx.notify();
    }

    fn end_move(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_none() {
            return;
        }
        if let Some(hwnd) = self.hwnd {
            if let Some(rect) = floating::window_rect(hwnd) {
                let clamped = positions::clamp_into(&floating::work_areas(), rect, (rect.0 + rect.2 / 2, rect.1 + rect.3 / 2));
                floating::place(cx.background_executor(), hwnd, clamped.0, clamped.1, clamped.2, clamped.3);
            }
            self.save_position(hwnd);
        }
        cx.notify();
    }

    fn begin_resize(&mut self, edge: &'static str, cx: &mut Context<Self>) {
        let Some(hwnd) = self.hwnd else { return };
        let Some(edge) = positions::parse_resize_edge(edge) else { return };
        let (Some(cursor), Some(rect)) = (floating::cursor(), floating::window_rect(hwnd)) else { return };
        let scale = floating::dpi_scale(hwnd);
        let min = ((layout::EXPANDED_MIN.0 * scale).round() as i32, (layout::EXPANDED_MIN.1 * scale).round() as i32);
        let max = floating::area_for_window(hwnd).map(|area| (area.width, area.height)).unwrap_or((rect.2, rect.3));
        self.resize = Some(ResizeDrag { edge, cursor, rect, min, max });
        self.transition = None;
        cx.notify();
    }

    fn resize_step(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.resize.as_ref() else { return };
        let Some(hwnd) = self.hwnd else { return };
        let Some(cursor) = floating::cursor() else { return };
        let rect = positions::resized_rect(drag.rect, (cursor.0 - drag.cursor.0, cursor.1 - drag.cursor.1), drag.edge, drag.min, drag.max);
        let clamped = positions::clamp_into(&floating::work_areas(), rect, (rect.0 + rect.2 / 2, rect.1 + rect.3 / 2));
        floating::place(cx.background_executor(), hwnd, clamped.0, clamped.1, clamped.2, clamped.3);
        cx.notify();
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if self.resize.take().is_none() {
            return;
        }
        if let Some(hwnd) = self.hwnd {
            self.save_position(hwnd);
        }
        cx.notify();
    }

    /// The size presets (`set_companion_size_preset`).
    pub fn apply_preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        let (target, large) = preset.target(self.settings.saved_size());
        if preset == Preset::Default {
            self.settings.size = None;
        }
        self.settings.maximised = large;
        if let Some(hwnd) = self.hwnd {
            let scale = floating::dpi_scale(hwnd);
            let area = floating::area_for_window(hwnd).map(|area| area.logical(scale)).unwrap_or((1920.0, 1080.0));
            let (width, height) = layout::clamp_size(target.0, target.1, area.0, area.1);
            if let Some(rect) = floating::window_rect(hwnd) {
                let size = ((width * scale).round() as i32, (height * scale).round() as i32);
                let centre = rect.0 + rect.2 / 2;
                let bottom = rect.1 + rect.3;
                let placed = positions::clamp_into(&floating::work_areas(), (centre - size.0 / 2, bottom - size.1, size.0, size.1), (centre, bottom));
                floating::place(cx.background_executor(), hwnd, placed.0, placed.1, placed.2, placed.3);
            }
            self.save_position(hwnd);
        }
        self.fsm.force_home(Instant::now());
        self.after_mode_change(cx);
    }

    /// A file dropped on the island goes to the picked conversation, or opens one
    /// (`companionDrop.ts`): the React app saves the path as a reference chip and
    /// sends it with the next message; here the drop sends one short prompt.
    fn dropped_paths(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let Some(path) = paths.first() else { return };
        self.drop = drop::advance_drop_zone(self.drop, drop::DropEvent::Drop);
        self.drop_seen_at = None;
        self.sound.play(CompanionCue::Pop);
        self.shared.update(cx, |shared, cx| shared.send_prompt(drop::drop_prompt(&path.to_string_lossy()), cx));
        cx.notify();
    }

    // ---- shared view pieces ---------------------------------------------------

    fn status_of(&self, shared: &Shared, agent: Option<&Agent>, session: Option<&Session>) -> Status {
        let provider = agent.and_then(|agent| shared.snapshot().and_then(|snapshot| snapshot.provider_of(agent)));
        let runtime_status = agent.and_then(|agent| {
            shared
                .snapshot()
                .and_then(|snapshot| snapshot.runtimes.iter().find(|runtime| runtime.id == agent.runtime_id))
                .and_then(|runtime| runtime.status.clone())
        });
        let permission_pending = session
            .map(|session| shared.snapshot().map(|snapshot| snapshot.pending_permission(&session.id).is_some()).unwrap_or(false))
            .unwrap_or(false);
        let input = status::StatusInput {
            connected: shared.connected(),
            runtime_status: runtime_status.as_deref(),
            provider: provider.as_deref(),
            session,
            permission_pending,
            composing: false,
            now_ms: millis() as i64,
        };
        status::derive(&input)
    }

    fn mark_in(&self, agent: &Agent, size: f32, colors: Colors, cx: &App) -> AnyElement {
        let provider = self.shared.read(cx).snapshot().and_then(|snapshot| snapshot.provider_of(agent));
        Mark::for_agent(size, art::agent_color(&agent.color), agent.icon(), provider.as_deref(), agent.name.clone(), colors).render()
    }

    fn status_dot(&self, state: BlobState, colors: Colors) -> AnyElement {
        let color = match state {
            BlobState::Waiting | BlobState::Working => colors.status_working,
            BlobState::Idle => colors.status_success,
            BlobState::Offline => colors.text_3,
        };
        div().flex_none().size(px(7.0)).rounded(px(4.0)).bg(color).into_any_element()
    }

    fn peers(&self, cx: &App, limit: usize) -> Vec<(Agent, BlobState)> {
        let shared = self.shared.read(cx);
        let Some(snapshot) = shared.snapshot() else { return Vec::new() };
        let focused = shared.focused_agent().map(|agent| agent.id.clone());
        let mut rows: Vec<(Agent, BlobState)> = snapshot
            .agents
            .iter()
            .filter(|agent| agent.is_employee() && focused.as_deref() != Some(agent.id.as_str()))
            .map(|agent| (agent.clone(), attention::blob_state(snapshot, agent, shared.connected())))
            .collect();
        let rank = |state: BlobState| match state {
            BlobState::Waiting => 0,
            BlobState::Working => 1,
            BlobState::Idle => 2,
            BlobState::Offline => 3,
        };
        rows.sort_by_key(|(agent, state)| (rank(*state), agent.name.to_lowercase()));
        rows.truncate(limit);
        rows
    }

    fn needs_you(&mut self, count: usize, compact: bool, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let text = attention::needs_you_text(count)?;
        let shown = if compact { format!("{count}") } else { text };
        Some(
            div()
                .id("needs-you")
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(if compact { 22.0 } else { 24.0 }))
                .px(px(8.0))
                .rounded(px(theme::RADIUS_SM))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover))
                .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| {
                    let next = {
                        let shared = companion.shared.read(cx);
                        let waiting: Vec<Agent> = shared
                            .snapshot()
                            .map(|snapshot| attention::waiting_agents(snapshot, shared.connected()).into_iter().cloned().collect())
                            .unwrap_or_default();
                        let focused = shared.focused_agent().map(|agent| agent.id.clone());
                        attention::next_waiting(&waiting.iter().collect::<Vec<_>>(), focused.as_deref()).map(|agent| agent.id.clone())
                    };
                    if let Some(next) = next {
                        companion.focus_agent(next, cx);
                    }
                }))
                .child(self.status_dot(BlobState::Waiting, colors))
                .child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_working).child(SharedString::from(shown)))
                .into_any_element(),
        )
    }

    fn close_button(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("companion-collapse")
            .flex_none()
            .size(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .cursor_pointer()
            .hover(move |style| style.bg(colors.hover))
            .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| companion.collapse(cx)))
            .child(art::glyph(art::Glyph::CHEVRON_DOWN, 14.0, colors.text_2))
            .into_any_element()
    }

    fn open_in_bitbots_button(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("companion-open")
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(26.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .cursor_pointer()
            .bg(colors.raised)
            .hover(move |style| style.bg(colors.hover))
            .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| companion.open_in_bitbots(cx)))
            .child(art::glyph(art::Glyph::CHEVRON, 12.0, colors.text_2))
            .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).child(SharedString::from("Open in Bitbots")))
            .into_any_element()
    }

    // ---- the island -----------------------------------------------------------

    fn render_welcome(&mut self, colors: Colors, agent: Option<&Agent>, cx: &mut Context<Self>) -> AnyElement {
        // A simplified greeting: the portrait grows in and waves, and the name
        // and role fade up beside it. `greetingScene.ts` draws a whole scene
        // (particles, halo, badge) in a canvas; GPUI has no canvas element at
        // this revision, so this is the portrait and the wave, as the brief
        // allows.
        let elapsed = self.greeting_started.elapsed().as_secs_f32();
        let grow = (elapsed / 0.45).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - grow).powi(3);
        let size = 20.0 + 36.0 * eased;
        let wave = if elapsed > 1.36 && elapsed < 2.6 {
            (elapsed * std::f32::consts::TAU * 2.5).sin() * 2.0
        } else {
            0.0
        };
        let name = agent.map(|agent| agent.name.clone()).unwrap_or_else(|| "Bitbots".to_owned());
        let role = agent.map(|agent| agent.role_or_default().to_owned()).unwrap_or_else(|| "Your team".to_owned());
        let mut mark = div().flex_none().flex().items_center().justify_center().size(px(64.0));
        if let Some(agent) = agent {
            mark = mark.child(self.mark_in(agent, size, colors, cx));
        }
        div()
            .id("companion-welcome")
            .size_full()
            .flex()
            .items_center()
            .gap(px(16.0))
            .px(px(26.0))
            .rounded(px(20.0))
            .bg(colors.base)
            .border_1()
            .border_color(colors.line)
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseDownEvent, _window, cx| companion.begin_move(cx)))
            .on_mouse_move(cx.listener(|companion, _: &gpui::MouseMoveEvent, _window, cx| {
                if companion.drag.is_some() {
                    companion.move_window(cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseUpEvent, _window, cx| companion.end_move(cx)))
            .child(div().flex_none().size(px(64.0)).flex().items_center().justify_center().mt(px(wave + 2.0)).child(mark))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w(px(0.0))
                    .child(div().truncate().text_size(px(theme::FS_XL)).text_color(colors.text_1).child(SharedString::from(name)))
                    .child(div().truncate().text_size(px(theme::FS_SM)).text_color(colors.text_2).child(SharedString::from(role)))
                    .child(
                        div()
                            .mt(px(6.0))
                            .flex()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("welcome-open")
                                    .flex()
                                    .items_center()
                                    .h(px(28.0))
                                    .px(px(12.0))
                                    .rounded(px(8.0))
                                    .bg(colors.selected)
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(colors.selected_strong))
                                    .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| companion.expand(cx)))
                                    .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_1).child(SharedString::from("Open the companion"))),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_island(&self, colors: Colors, phase: IslandPhase, level: f32, warming: bool, error: &str, seconds: u64) -> AnyElement {
        // The dictation island inside the companion: a red dot and timer, the
        // level bars, and the one message it ever says (`DictationIsland.tsx`).
        let shown = if phase == IslandPhase::Error { crate::dictation::copy::friendly_error(error) } else { String::new() };
        let bars = level_bars(level, phase);
        let mut row = div().flex().items_center().gap(px(10.0));
        row = row.child(
            div()
                .flex_none()
                .size(px(8.0))
                .rounded(px(4.0))
                .bg(colors.status_danger),
        );
        row = row.child(div().text_size(px(theme::FS_XS)).text_color(colors.text_1).child(SharedString::from(format_seconds(seconds))));
        if let Some(note) = phase.note(warming) {
            row = row.child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from(note)));
        }
        row = row.child(
            div()
                .flex()
                .flex_1()
                .justify_end()
                .items_end()
                .gap(px(2.0))
                .children(bars.into_iter().map(|height| div().flex_none().w(px(3.0)).h(px(height)).rounded(px(2.0)).bg(colors.text_3))),
        );
        let mut card = div()
            .absolute()
            .left(px(10.0))
            .right(px(10.0))
            .bottom(px(10.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .rounded(px(14.0))
            .bg(colors.panel)
            .border_1()
            .border_color(colors.line)
            .child(row);
        if !shown.is_empty() {
            card = card.child(div().text_size(px(theme::FS_2XS)).text_color(colors.status_danger).child(SharedString::from(shown)));
        }
        card.into_any_element()
    }

    // ---- the expanded layout --------------------------------------------------

    #[allow(clippy::too_many_arguments)] // the expanded layout reads from the whole view
    fn render_expanded(
        &mut self,
        colors: Colors,
        window: &mut Window,
        agent: Option<&Agent>,
        session: Option<&Session>,
        approvals: &[Approval],
        waiting: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = window;
        let shared = self.shared.read(cx);
        let connected = shared.connected();
        let focused_id = agent.map(|agent| agent.id.clone());
        let roster: Vec<(Agent, BlobState)> = shared
            .snapshot()
            .map(|snapshot| {
                let mut rows: Vec<(Agent, BlobState)> = snapshot
                    .agents
                    .iter()
                    .filter(|agent| agent.is_employee())
                    .map(|agent| (agent.clone(), attention::blob_state(snapshot, agent, connected)))
                    .collect();
                rows.sort_by(|a, b| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase()));
                rows
            })
            .unwrap_or_default();
        let project = session.and_then(|session| session.project_name.clone());
        let status = self.status_of(shared, agent, session);
        let bot_name = agent.map(|agent| agent.name.clone()).unwrap_or_else(|| "Bitbots".to_owned());
        let conversations: Vec<(String, String)> = shared
            .snapshot()
            .zip(focused_id.as_deref())
            .map(|(snapshot, agent_id)| {
                snapshot
                    .conversations_for(agent_id)
                    .into_iter()
                    .take(30)
                    .map(|session| (session.id.clone(), session.title_or_default().to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let current_id = shared.session_id().map(str::to_owned);

        let mut shell = div()
            .id("companion-shell")
            .size_full()
            .flex()
            .rounded(px(18.0))
            .bg(colors.base)
            .border_1()
            .border_color(colors.line)
            .overflow_hidden();

        // The roster: Haze keeps one column of rows, 176 px wide.
        let mut brand = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .h(px(26.0))
            .px(px(6.0))
            .on_mouse_down(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseDownEvent, _window, cx| companion.begin_move(cx)))
            .on_mouse_move(cx.listener(|companion, _: &gpui::MouseMoveEvent, _window, cx| {
                if companion.drag.is_some() {
                    companion.move_window(cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseUpEvent, _window, cx| companion.end_move(cx)))
            .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_2).child(SharedString::from("Bitbots")));
        if let Some(pill) = self.needs_you(waiting, true, colors, cx) {
            brand = brand.child(pill);
        }
        let mut bots = div().flex().flex_col().gap(px(1.0)).mt(px(4.0)).min_h(px(0.0)).overflow_hidden();
        for (row_agent, state) in &roster {
            let focused = focused_id.as_deref() == Some(row_agent.id.as_str());
            bots = bots.child(self.roster_row(row_agent, *state, focused, colors, cx));
        }
        let roster_column = div()
            .flex()
            .flex_col()
            .w(px(176.0))
            .flex_none()
            .h_full()
            .px(px(6.0))
            .py(px(8.0))
            .bg(colors.sidebar)
            .border_r_1()
            .border_color(colors.line)
            .child(brand)
            .child(bots);

        // The head: the bot's name, what it is doing, and the picker.
        let mut head = div()
            .id("companion-head")
            .flex()
            .items_center()
            .gap(px(10.0))
            .h(px(44.0))
            .flex_none()
            .px(px(14.0))
            .border_b_1()
            .border_color(colors.line)
            .on_mouse_down(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseDownEvent, _window, cx| companion.begin_move(cx)))
            .on_mouse_move(cx.listener(|companion, _: &gpui::MouseMoveEvent, _window, cx| {
                if companion.drag.is_some() {
                    companion.move_window(cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseUpEvent, _window, cx| companion.end_move(cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(div().truncate().text_size(px(13.0)).text_color(colors.text_1).child(SharedString::from(bot_name)))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_3)
                            .child(SharedString::from(format!("{} · {}", status.label, project.unwrap_or_else(|| "No project".to_owned())))),
                    ),
            );
        if !conversations.is_empty() {
            let picker_open = self.picker_open;
            head = head.child(
                div()
                    .id("companion-picker")
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .h(px(26.0))
                    .px(px(8.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(colors.hover))
                    .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| {
                        companion.picker_open = !companion.picker_open;
                        cx.notify();
                    }))
                    .child(art::glyph(art::Glyph::MESSAGES, 13.0, colors.text_2))
                    .child(
                        div()
                            .max_w(px(180.0))
                            .truncate()
                            .text_size(px(theme::FS_2XS))
                            .text_color(if picker_open { colors.text_1 } else { colors.text_2 })
                            .child(SharedString::from(
                                current_id
                                    .as_ref()
                                    .and_then(|id| conversations.iter().find(|(session, _)| session == id).map(|(_, title)| title.clone()))
                                    .unwrap_or_else(|| "No conversations yet".to_owned()),
                            )),
                    ),
            );
        }
        head = head
            .child(self.size_button(colors, cx))
            .child(self.open_in_bitbots_button(colors, cx))
            .child(self.close_button(colors, cx));

        let main = div().flex().flex_col().flex_1().min_w(px(0.0)).h_full().child(head).child(
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .children(self.picker_open.then(|| self.render_picker(&conversations, current_id.as_deref(), colors, cx)))
                .children(
                    approvals
                        .get(self.queue_index % approvals.len().max(1))
                        .map(|approval| self.render_approval(approval, approvals.len(), colors, cx)),
                )
                .child(self.render_messages(session, colors))
                .children(self.render_drop_card(colors, agent))
                .child(self.render_composer(colors, cx)),
        );

        shell = shell.child(roster_column).child(main);
        let mut wrapped = div().relative().size_full().child(shell);
        if let Some(handles) = self.render_resize_handles(colors, cx) {
            wrapped = wrapped.child(handles);
        }
        wrapped.into_any_element()
    }

    fn roster_row(&mut self, agent: &Agent, state: BlobState, focused: bool, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let id = agent.id.clone();
        let state_text = match state {
            BlobState::Waiting => "needs",
            BlobState::Working => "working",
            BlobState::Idle => "",
            BlobState::Offline => "offline",
        };
        let mut row = div()
            .id(SharedString::from(format!("roster-{}", agent.id)))
            .flex()
            .items_center()
            .gap(px(9.0))
            .h(px(30.0))
            .px(px(8.0))
            .rounded(px(9.0))
            .cursor_pointer()
            .when(focused, |this| this.bg(colors.selected))
            .hover(move |style| style.bg(colors.hover))
            .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.focus_agent(id.clone(), cx)))
            .child(self.mark_in(agent, 20.0, colors, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(12.5))
                    .text_color(if state == BlobState::Offline { colors.text_3 } else { colors.text_1 })
                    .child(SharedString::from(agent.name.clone())),
            );
        if !state_text.is_empty() {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(px(10.5))
                    .text_color(if state == BlobState::Waiting { colors.status_working } else { colors.text_3 })
                    .child(SharedString::from(state_text)),
            );
        }
        row.into_any_element()
    }

    fn render_picker(&mut self, conversations: &[(String, String)], current: Option<&str>, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("companion-picker-list")
            .absolute()
            .top(px(4.0))
            .right(px(8.0))
            .w(px(260.0))
            .max_h(px(220.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .p(px(6.0))
            .rounded(px(12.0))
            .bg(colors.overlay)
            .border_1()
            .border_color(colors.line_strong);
        if conversations.is_empty() {
            list = list.child(div().p(px(8.0)).text_size(px(theme::FS_2XS)).text_color(colors.text_3).child(SharedString::from("No conversations yet")));
        }
        for (id, title) in conversations {
            let session_id = id.clone();
            let chosen = current == Some(id.as_str());
            list = list.child(
                div()
                    .id(SharedString::from(format!("picker-{}", id)))
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px(px(8.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .when(chosen, |this| this.bg(colors.selected))
                    .hover(move |style| style.bg(colors.hover))
                    .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.pick_session(session_id.clone(), cx)))
                    .child(div().flex_1().min_w(px(0.0)).truncate().text_size(px(theme::FS_XS)).text_color(colors.text_1).child(SharedString::from(title.clone()))),
            );
        }
        list.into_any_element()
    }

    fn render_approval(&mut self, approval: &Approval, total: usize, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        // "2 need you" is the badge; here the request itself, with the queue
        // position, Allow and Deny, and "Later" (`CompanionAttention.tsx`).
        let title = approval.title.clone();
        let detail = approval.detail.clone();
        let allow = approval.clone();
        let deny = approval.clone();
        let later = approval.clone();
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .m(px(10.0))
            .mb(px(0.0))
            .p(px(10.0))
            .rounded(px(14.0))
            .bg(colors.raised)
            .border_1()
            .border_color(colors.status_working);
        if total > 1 {
            let index = self.queue_index % total;
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .id("queue-prev")
                            .flex_none()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| {
                                companion.queue_index = attention::step_index(companion.queue_index, total, false);
                                cx.notify();
                            }))
                            .child(art::glyph(art::Glyph::ARROW_LEFT, 12.0, colors.text_2)),
                    )
                    .child(
                        div()
                            .text_size(px(theme::FS_2XS))
                            .text_color(colors.text_2)
                            .child(SharedString::from(format!("{} of {total}", index + 1))),
                    )
                    .child(
                        div()
                            .id("queue-next")
                            .flex_none()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(move |style| style.bg(colors.hover))
                            .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| {
                                companion.queue_index = attention::step_index(companion.queue_index, total, true);
                                cx.notify();
                            }))
                            .child(art::glyph(art::Glyph::CHEVRON, 12.0, colors.text_2)),
                    ),
            );
        }
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(self.status_dot(BlobState::Waiting, colors))
                .child(div().flex_1().truncate().text_size(px(theme::FS_SM)).text_color(colors.text_1).child(SharedString::from(title))),
        );
        if !detail.trim().is_empty() {
            card = card.child(
                div()
                    .max_h(px(54.0))
                    .overflow_hidden()
                    .text_size(px(theme::FS_2XS))
                    .text_color(colors.text_2)
                    .child(SharedString::from(detail)),
            );
        }
        card = card.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .id("approval-allow")
                        .h(px(26.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(999.0))
                        .bg(colors.accent)
                        .cursor_pointer()
                        .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.allow(&allow, cx)))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_inverse).child(SharedString::from("Allow"))),
                )
                .child(
                    div()
                        .id("approval-deny")
                        .h(px(26.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .rounded(px(999.0))
                        .bg(colors.selected)
                        .cursor_pointer()
                        .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.deny(&deny, cx)))
                        .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_1).child(SharedString::from("Deny"))),
                )
                .child(
                    div()
                        .id("approval-later")
                        .ml(px(2.0))
                        .h(px(26.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .rounded(px(999.0))
                        .border_1()
                        .border_color(colors.line_strong)
                        .cursor_pointer()
                        .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.later(&later, cx)))
                        .child(div().text_size(px(theme::FS_2XS)).text_color(colors.text_2).child(SharedString::from("Later"))),
                )
                .child(div().flex_1())
                .child(self.open_in_bitbots_button(colors, cx)),
        );
        card.into_any_element()
    }

    fn render_messages(&mut self, session: Option<&Session>, colors: Colors) -> AnyElement {
        let mut log = div().id("companion-log").flex().flex_col().gap(px(6.0)).flex_1().min_h(px(0.0)).px(px(12.0)).py(px(8.0)).overflow_hidden();
        let Some(session) = session else {
            return log
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from("No conversation yet. Write below and the bot starts one.")))
                .into_any_element();
        };
        let rows: Vec<&crate::state::Message> = session.messages.iter().filter(|message| !message.is_thinking()).collect();
        if rows.is_empty() {
            return log
                .child(div().text_size(px(theme::FS_XS)).text_color(colors.text_3).child(SharedString::from("No messages yet.")))
                .into_any_element();
        }
        // The last few lines only: the companion is a glance, not the transcript.
        let start = rows.len().saturating_sub(6);
        for message in &rows[start..] {
            let user = matches!(message.role(), "user");
            let text = message.text().trim();
            if text.is_empty() {
                continue;
            }
            let shown: String = text.chars().take(600).collect();
            log = log.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .when(user, |this| this.items_end())
                    .child(
                        div()
                            .max_w(px(380.0))
                            .px(px(9.0))
                            .py(px(6.0))
                            .rounded(px(theme::RADIUS_MD))
                            .bg(if user { colors.bubble_user } else { colors.raised })
                            .text_size(px(theme::FS_XS))
                            .text_color(colors.text_1)
                            .child(SharedString::from(shown)),
                    ),
            );
        }
        log.into_any_element()
    }

    fn render_drop_card(&mut self, colors: Colors, agent: Option<&Agent>) -> Option<AnyElement> {
        if !drop::drop_zone_active(self.drop) {
            return None;
        }
        let name = agent.map(|agent| agent.name.clone()).unwrap_or_else(|| "your bot".to_owned());
        Some(
            div()
                .absolute()
                .inset(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(16.0))
                .bg(colors.raised)
                .border_1()
                .border_color(colors.status_success)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(theme::FS_SM))
                                .text_color(colors.status_success)
                                .child(SharedString::from(format!("Drop to hand this file to {name}"))),
                        )
                        .child(
                            div()
                                .text_size(px(theme::FS_2XS))
                                .text_color(colors.text_3)
                                .child(SharedString::from("Bitbots checks the path only. It never reads file contents.")),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_composer(&mut self, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_none()
            .p(px(10.0))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(6.0))
                    .p(px(6.0))
                    .rounded(px(theme::RADIUS_LG))
                    .bg(colors.input)
                    .border_1()
                    .border_color(colors.line)
                    .child(div().flex_1().min_w(px(0.0)).child(self.composer.clone()))
                    .child(
                        div()
                            .id("companion-send")
                            .flex_none()
                            .size(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .bg(colors.accent)
                            .cursor_pointer()
                            .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| companion.send(cx)))
                            .child(art::glyph(art::Glyph::CHECK, 13.0, colors.text_inverse)),
                    ),
            )
            .into_any_element()
    }

    fn render_resize_handles(&mut self, colors: Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Eight strips: the frameless window draws its own edges. A motionless
        // pixel of tint keeps each strip a real, hit-testable element.
        let edges: [(&'static str, f32, f32, f32, f32); 8] = [
            ("n", 0.0, 0.0, 0.0, 6.0),
            ("s", 0.0, 0.0, 0.0, 6.0),
            ("w", 0.0, 0.0, 6.0, 0.0),
            ("e", 0.0, 0.0, 6.0, 0.0),
            ("nw", 0.0, 0.0, 12.0, 12.0),
            ("ne", 0.0, 0.0, 12.0, 12.0),
            ("sw", 0.0, 0.0, 12.0, 12.0),
            ("se", 0.0, 0.0, 12.0, 12.0),
        ];
        let mut layer = div().absolute().inset_0();
        for (edge, _, _, width, height) in edges {
            let mut strip = div()
                .id(SharedString::from(format!("resize-{edge}")))
                .absolute()
                .bg(rgba(0x00000001))
                .on_mouse_down(MouseButton::Left, cx.listener(move |companion, _: &gpui::MouseDownEvent, _window, cx| {
                    companion.begin_resize(edge, cx);
                }))
                .on_mouse_move(cx.listener(|companion, _: &gpui::MouseMoveEvent, _window, cx| {
                    if companion.resize.is_some() {
                        companion.resize_step(cx);
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseUpEvent, _window, cx| companion.end_resize(cx)));
            strip = match edge {
                "n" => strip.top(px(0.0)).left(px(10.0)).right(px(10.0)).h(px(6.0)),
                "s" => strip.bottom(px(0.0)).left(px(10.0)).right(px(10.0)).h(px(6.0)),
                "w" => strip.left(px(0.0)).top(px(10.0)).bottom(px(10.0)).w(px(6.0)),
                "e" => strip.right(px(0.0)).top(px(10.0)).bottom(px(10.0)).w(px(6.0)),
                "nw" => strip.top(px(0.0)).left(px(0.0)).w(px(width)).h(px(height)),
                "ne" => strip.top(px(0.0)).right(px(0.0)).w(px(width)).h(px(height)),
                "sw" => strip.bottom(px(0.0)).left(px(0.0)).w(px(width)).h(px(height)),
                _ => strip.bottom(px(0.0)).right(px(0.0)).w(px(width)).h(px(height)),
            };
            layer = layer.child(strip);
        }
        let _ = colors;
        Some(layer.into_any_element())
    }
}

fn millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// "0:07": the island's elapsed time.
pub fn format_seconds(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The island's eleven bars, newest on the right.
pub fn level_bars(level: f32, phase: IslandPhase) -> Vec<f32> {
    let value = (level.max(0.0).powf(0.58) * 2.1).min(1.0);
    let active = if phase == IslandPhase::Listening { value } else { 0.25 };
    (0..11)
        .map(|index| {
            let taper = 0.55 + 0.45 * (((index + 1) as f32) / 12.0 * std::f32::consts::PI).sin();
            (3.0 + 15.0 * active * taper).max(3.0)
        })
        .collect()
}

/// Haze's companion palette, from `ui/themes/haze.css` (`.companion-surface` and
/// its `[data-surface-theme="light"]` block).
pub fn companion_colors(dark: bool) -> Colors {
    if dark {
        Colors {
            dark: true,
            base: rgb(0x202124),
            backdrop_fallback: rgba(0x202124f2),
            sidebar: rgb(0x242528),
            panel: rgb(0x242528),
            raised: rgb(0x2a2b2f),
            overlay: rgb(0x2c2d31),
            input: rgb(0x2f3034),
            hover: rgb(0x35363b),
            selected: rgb(0x3c3d42),
            selected_strong: rgb(0x46474d),
            avatar: rgba(0xffffff12),
            bubble_user: rgb(0x3a3b40),
            text_1: rgb(0xf3f3f2),
            text_2: rgb(0xc9c9c7),
            text_3: rgb(0xa9a9a6),
            placeholder: rgb(0xa9a9a6),
            text_inverse: rgb(0x202124),
            line: rgb(0x3a3b40),
            line_strong: rgb(0x5a5b61),
            status_working: rgb(0xffae42),
            status_success: rgb(0x3ddc97),
            status_danger: rgb(0xff6b6b),
            status_info: rgb(0x6ea8ff),
            accent: rgb(0xf3f3f2),
            scrim: rgba(0x00000052),
        }
    } else {
        Colors {
            dark: false,
            base: rgb(0xf4f2ee),
            backdrop_fallback: rgba(0xf4f2eef2),
            sidebar: rgb(0xeeebe6),
            panel: rgb(0xeeebe6),
            raised: rgb(0xfbfaf8),
            overlay: rgb(0xffffff),
            input: rgb(0xe9e6e1),
            hover: rgb(0xe6e3de),
            selected: rgb(0xdedad4),
            selected_strong: rgb(0xd3cfc8),
            avatar: rgba(0x1414140f),
            bubble_user: rgb(0xe6e3de),
            text_1: rgb(0x1b1b1a),
            text_2: rgb(0x4d4c49),
            text_3: rgb(0x5c5b57),
            placeholder: rgb(0x5c5b57),
            text_inverse: rgb(0xffffff),
            line: rgb(0xd2cec7),
            line_strong: rgb(0xb3afa8),
            status_working: rgb(0x7a4d00),
            status_success: rgb(0x17613f),
            status_danger: rgb(0xa82d43),
            status_info: rgb(0x2f6bd6),
            accent: rgb(0x1b1b1a),
            scrim: rgba(0x00000052),
        }
    }
}

impl Render for Companion {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.adopt_window(window, cx);
        let colors = self.colors();
        let mode = self.fsm.mode();
        if mode == Mode::Hidden {
            return div().size_full().into_any_element();
        }
        let (agent, session, approvals, waiting) = {
            let shared = self.shared.read(cx);
            let agent = shared.focused_agent().cloned();
            let session = shared.session_of(shared.session_id()).cloned();
            let approvals = shared
                .snapshot()
                .map(|snapshot| attention::approvals(snapshot, &self.held, Instant::now()))
                .unwrap_or_default();
            let waiting = shared
                .snapshot()
                .map(|snapshot| attention::waiting_agents(snapshot, shared.connected()).len())
                .unwrap_or(0);
            (agent, session, approvals, waiting)
        };

        let mut root = div()
            .relative()
            .size_full()
            .font_family(theme::UI_FONT)
            .text_color(colors.text_1)
            .text_size(px(theme::FS_SM));

        match mode {
            Mode::Hidden => {}
            Mode::Welcome => {
                let welcome = self.render_welcome(colors, agent.as_ref(), cx);
                root = root.child(welcome);
            }
            Mode::Petit => {
                let peek_text = self
                    .peek
                    .as_ref()
                    .filter(|peek| peek::peek_visible(peek, millis()))
                    .map(|_| peek::peek_text(&self.peek_names, "bots"));
                let status = {
                    let shared = self.shared.read(cx);
                    self.status_of(shared, agent.as_ref(), session.as_ref())
                };
                let label = peek_text.clone().unwrap_or_else(|| status.label.clone());
                let name = agent.as_ref().map(|agent| agent.name.clone()).unwrap_or_else(|| "Bitbots".to_owned());
                let mut bar = div()
                    .id("companion-compact")
                    .size_full()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(12.0))
                    .rounded(px(14.0))
                    .bg(colors.base)
                    .border_1()
                    .border_color(if drop::drop_zone_active(self.drop) { colors.status_success } else { colors.line })
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseDownEvent, _window, cx| companion.begin_move(cx)))
                    .on_mouse_move(cx.listener(|companion, _: &gpui::MouseMoveEvent, _window, cx| {
                        if companion.drag.is_some() {
                            companion.move_window(cx);
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|companion, _: &gpui::MouseUpEvent, _window, cx| companion.end_move(cx)))
                    .on_hover(cx.listener(|companion, hovered: &bool, _window, cx| companion.hover_changed(*hovered, cx)))
                    .on_drag_move::<ExternalPaths>(cx.listener(|companion, _: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                        companion.drop = drop::advance_drop_zone(companion.drop, drop::DropEvent::Enter);
                        companion.drop_seen_at = Some(Instant::now());
                        cx.notify();
                    }))
                    .on_drop::<ExternalPaths>(cx.listener(|companion, paths: &ExternalPaths, _window, cx| {
                        companion.dropped_paths(paths.paths(), cx);
                    }));
                if let Some(agent) = agent.as_ref() {
                    bar = bar.child(
                        div()
                            .flex_none()
                            .size(px(30.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.0))
                            .bg(colors.raised)
                            .child(self.mark_in(agent, 20.0, colors, cx)),
                    );
                }
                let mut copy = div()
                    .id("companion-name")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap(px(1.0))
                    .cursor_pointer()
                    .on_click(cx.listener(|companion, _: &gpui::ClickEvent, _window, cx| companion.expand(cx)))
                    .child(div().truncate().text_size(px(13.0)).text_color(colors.text_1).child(SharedString::from(name)));
                copy = copy.child(
                    div()
                        .truncate()
                        .text_size(px(theme::FS_2XS))
                        .text_color(if peek_text.is_some() { colors.status_success } else { colors.text_2 })
                        .child(SharedString::from(label)),
                );
                bar = bar.child(copy);
                if let Some(pill) = self.needs_you(waiting, true, colors, cx) {
                    bar = bar.child(pill);
                }
                let peers = self.peers(cx, COMPACT_PEERS);
                if !peers.is_empty() {
                    let mut grid = div().flex().flex_none().items_center().gap(px(4.0));
                    for (peer, state) in peers {
                        let id = peer.id.clone();
                        let mut tile = div()
                            .id(SharedString::from(format!("peer-{}", peer.id)))
                            .flex_none()
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.0))
                            .bg(colors.raised)
                            .cursor_pointer()
                            .on_click(cx.listener(move |companion, _: &gpui::ClickEvent, _window, cx| companion.focus_agent(id.clone(), cx)))
                            .child(self.mark_in(&peer, 16.0, colors, cx));
                        if state == BlobState::Waiting {
                            tile = tile.border_1().border_color(colors.status_working);
                        }
                        grid = grid.child(tile);
                    }
                    bar = bar.child(grid);
                }
                bar = bar.child(self.close_button(colors, cx));
                root = root.child(bar);
                // The drop card replaces the bar's content while a drag is on it.
                if let Some(card) = self.render_drop_card(colors, agent.as_ref()) {
                    root = root.child(card);
                }
            }
            Mode::Home => {
                let expanded = self.render_expanded(colors, window, agent.as_ref(), session.as_ref(), &approvals, waiting, cx);
                let dictation = self.dictation.read(cx);
                let island = if dictation.owner == DictationOwner::Companion && dictation.companion_visible() {
                    dictation.phase.island().map(|phase| {
                        let (level, warming, error) = (dictation.level, dictation.warming, dictation.error.clone());
                        let seconds = dictation.display_seconds();
                        (phase, level, warming, error, seconds)
                    })
                } else {
                    None
                };
                match island {
                    Some((phase, level, warming, error, seconds)) => {
                        let card = self.render_island(colors, phase, level, warming, &error, seconds);
                        root = root.child(div().relative().size_full().child(expanded).child(card));
                    }
                    None => root = root.child(expanded),
                }
            }
        }
        root.into_any_element()
    }
}

/// Open the companion window: `WindowKind::PopUp` is `WS_EX_TOOLWINDOW`, so it is
/// frameless and out of the taskbar; the background is transparent so the
/// capsule's rounded corners cut out; Haze paints its own opaque smoke inside.
pub fn open(
    cx: &mut App,
    shared: Entity<Shared>,
    dictation: Entity<Dictation>,
    sound: std::sync::Arc<SoundEngine>,
    settings: CompanionSettings,
) -> gpui::Result<WindowHandle<Companion>> {
    let bounds = launch_bounds(&settings, cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            // Shown at creation, because a window that is never painted never
            // runs its first render: `adopt_window` is where the view learns its
            // own `HWND`, and every later place, hide or show depends on it.
            show: true,
            kind: WindowKind::PopUp,
            is_movable: true,
            is_resizable: false,
            is_minimizable: false,
            display_id: None,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: None,
            window_min_size: None,
            window_decorations: None,
            tabbing_identifier: None,
        },
        move |window, cx| {
            let appearance = window.appearance();
            let system_is_light = appearance != WindowAppearance::Dark && appearance != WindowAppearance::VibrantDark;
            cx.new(|cx| Companion::new(shared, dictation, sound, settings, system_is_light, cx))
        },
    )
}

/// Where the companion starts: the spot it was left at on the primary display,
/// else centred and a little above the bottom edge (`companion_launch_position`).
fn launch_bounds(settings: &CompanionSettings, cx: &mut App) -> Bounds<Pixels> {
    let areas = floating::work_areas();
    let area = areas.first().cloned().unwrap_or(WorkArea { x: 0, y: 0, width: 1920, height: 1080, name: String::new() });
    let _ = cx;
    // Before the window exists the DPI comes from the system, and every
    // rectangle here is in physical pixels; GPUI's bounds are logical.
    let scale = floating::system_scale();
    let (width, height) = if settings.visible && settings.greet_on_launch { layout::GREETING } else { layout::COMPACT };
    let physical = ((width * scale).round() as i32, (height * scale).round() as i32);
    let compact = (
        (layout::COMPACT.0 * scale).round() as i32,
        (layout::COMPACT.1 * scale).round() as i32,
    );
    let (x, y) = match settings.positions.get(&area.name) {
        Some(point) => positions::saved_spot(point, compact.0, compact.1, scale as f64, &area),
        None => positions::launch_spot(&area, compact.0, compact.1),
    };
    // The welcome island keeps the compact bar's bottom centre, so the window
    // grows upward instead of jumping when it shrinks to the bar.
    let (x, y) = if physical == compact {
        (x, y)
    } else {
        positions::last_launch_spot((x, y, compact.0, compact.1), physical.0, physical.1, &area)
    };
    Bounds::new(point(px(x as f32 / scale), px(y as f32 / scale)), Size { width: px(width), height: px(height) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_island_bars_are_short_and_never_vanishing() {
        let bars = level_bars(0.0, IslandPhase::Silent);
        assert_eq!(bars.len(), 11);
        assert!(bars.iter().all(|height| *height >= 3.0));
        let loud = level_bars(0.3, IslandPhase::Listening);
        assert!(loud.iter().any(|height| *height > 10.0), "a loud microphone moves the bars");
    }

    #[test]
    fn the_island_timer_reads_minutes_and_seconds() {
        assert_eq!(format_seconds(0), "0:00");
        assert_eq!(format_seconds(7), "0:07");
        assert_eq!(format_seconds(65), "1:05");
    }

    #[test]
    fn the_companion_palette_is_hazes_own_smoke() {
        let dark = companion_colors(true);
        assert_eq!(dark.base, rgb(0x202124));
        assert_eq!(dark.raised, rgb(0x2a2b2f));
        assert_eq!(dark.status_working, rgb(0xffae42));
        let light = companion_colors(false);
        assert_eq!(light.base, rgb(0xf4f2ee));
        assert_eq!(light.text_1, rgb(0x1b1b1a));
        // Haze's companion is opaque, not translucent: the window is transparent
        // and the capsule hides the desktop.
        assert!(dark.base.a > 0.99);
    }

    #[test]
    fn two_haze_palettes_agree_on_the_status_colours_they_share() {
        assert_ne!(companion_colors(true).status_success, companion_colors(false).status_success);
        assert_eq!(companion_colors(true).line, rgb(0x3a3b40));
    }
}
