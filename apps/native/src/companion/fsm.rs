//! The companion's open/close state machine.
//!
//! Ported from `src/blob/engine/island/fsm.ts` (`IslandStateMachine`) and the
//! Bitbots adapter in `src/blob/companionFsm.ts`. The four states are the same
//! (`hidden`, `petit`, `home`, `welcome`); `home-chat` in `companionLayout.ts` is
//! a view of `home`, not a state, so it is not modelled here.
//!
//! Bills for slices 1 and 2 stay the same: no timers, no window calls. A
//! deadline is stored and [`CompanionFsm::poll`] applies it, which is what makes
//! the transitions testable without sleeping.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::time::{Duration, Instant};

/// `FsmState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hidden,
    Petit,
    Home,
    Welcome,
}

impl Mode {
    /// The stored word, and the one the live check logs.
    pub fn key(self) -> &'static str {
        match self {
            Mode::Hidden => "hidden",
            Mode::Petit => "petit",
            Mode::Home => "home",
            Mode::Welcome => "welcome",
        }
    }

    /// Whether the window is on screen in this mode.
    pub fn visible(self) -> bool {
        self != Mode::Hidden
    }
}

/// What a deadline does when it fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// `petit` → `hidden` after the pointer has been away for a minute.
    PetitHide,
    /// `home` → `petit` after fifteen quiet seconds.
    HomeCollapse,
    /// `welcome` → `petit` once the greeting has played out.
    GreetCollapse,
}

#[derive(Clone, Debug)]
pub struct CompanionFsm {
    mode: Mode,
    /// An alert waiting for an answer stays open, even when the pointer leaves.
    pub pinned: bool,
    /// Whether the greeting has finished; the React adapter tracks this so a
    /// pointer that leaves early cannot cut the welcome short.
    greeted: bool,
    /// `home → petit`, seconds.
    pub home_to_petit: f64,
    /// `petit → hidden`, seconds.
    pub petit_to_hidden: f64,
    /// `welcome → petit` after the greeting, seconds. The companion rests a
    /// moment before it shrinks to the compact bar (`greetAutoCollapseDelay`).
    pub greet_auto: f64,
    /// `welcome → petit` while the pointer hovers the greeting, seconds.
    pub greet_hover: f64,
    pending: Option<(Pending, Instant)>,
}

impl Default for CompanionFsm {
    fn default() -> Self {
        Self::new()
    }
}

impl CompanionFsm {
    pub fn new() -> Self {
        Self {
            mode: Mode::Hidden,
            pinned: false,
            greeted: false,
            home_to_petit: 15.0,
            petit_to_hidden: 60.0,
            greet_auto: 1.4,
            greet_hover: 10.0,
            pending: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn greeted(&self) -> bool {
        self.greeted
    }

    /// When the next timed transition is due, for the window's ticker.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.map(|(_, at)| at)
    }

    // ---- inputs ---------------------------------------------------------------

    /// The window opened (or the companion was asked to show itself).
    pub fn launch(&mut self, now: Instant) {
        self.greeted = false;
        self.cancel();
        self.set(Mode::Welcome, now);
    }

    /// The welcome greeting finished playing (`greetComplete`).
    pub fn greet_complete(&mut self, now: Instant) {
        self.greeted = true;
        if self.mode != Mode::Welcome || self.is_pending(Pending::GreetCollapse) {
            return;
        }
        self.schedule(Pending::GreetCollapse, self.greet_auto, now);
    }

    pub fn mouse_entered(&mut self, now: Instant) {
        match self.mode {
            Mode::Hidden => {
                self.cancel();
                self.set(Mode::Petit, now);
            }
            Mode::Petit => self.clear(Pending::PetitHide),
            Mode::Home => self.clear(Pending::HomeCollapse),
            Mode::Welcome => self.schedule(Pending::GreetCollapse, self.greet_hover, now),
        }
    }

    pub fn mouse_left(&mut self, now: Instant) {
        match self.mode {
            Mode::Hidden => {}
            Mode::Petit => {
                // The Bitbots adapter cancels the compact auto-hide: a freely
                // positioned overlay must not vanish into an unknown spot.
            }
            Mode::Home => self.schedule(Pending::HomeCollapse, self.home_to_petit, now),
            Mode::Welcome => {
                // The greeting always plays to the end.
                self.clear(Pending::GreetCollapse);
                if self.greeted {
                    self.schedule(Pending::GreetCollapse, self.greet_auto, now);
                }
            }
        }
    }

    pub fn click(&mut self, now: Instant) {
        if self.mode != Mode::Petit {
            return;
        }
        self.cancel();
        self.set(Mode::Home, now);
    }

    /// Non-alert work: show the compact bar from hidden.
    pub fn reveal(&mut self, now: Instant) {
        if self.mode != Mode::Hidden {
            return;
        }
        self.cancel();
        self.set(Mode::Petit, now);
    }

    /// An alert or an explicit request opens straight to the expanded companion.
    pub fn force_home(&mut self, now: Instant) {
        self.pinned = true;
        self.cancel();
        self.set(Mode::Home, now);
    }

    /// Close the expanded companion. Respects `pinned` unless forced.
    pub fn force_petit(&mut self, force: bool, now: Instant) {
        if self.pinned && !force {
            return;
        }
        self.pinned = false;
        self.cancel();
        self.set(Mode::Petit, now);
    }

    pub fn force_hidden(&mut self, now: Instant) {
        self.pinned = false;
        self.cancel();
        self.set(Mode::Hidden, now);
    }

    /// Ctrl+Alt+B: hidden ↔ petit, and home → petit.
    pub fn toggle(&mut self, now: Instant) {
        match self.mode {
            Mode::Hidden => self.reveal(now),
            Mode::Petit => self.force_hidden(now),
            Mode::Welcome => self.force_petit(false, now),
            Mode::Home => self.force_petit(true, now),
        }
    }

    /// Apply any deadline that has come due. Returns whether the mode changed.
    pub fn poll(&mut self, now: Instant) -> bool {
        let Some((what, at)) = self.pending else { return false };
        if now < at {
            return false;
        }
        self.pending = None;
        match what {
            Pending::PetitHide if self.mode == Mode::Petit => {
                self.mode = Mode::Hidden;
                true
            }
            Pending::HomeCollapse if self.mode == Mode::Home => {
                if self.pinned {
                    return false;
                }
                self.mode = Mode::Petit;
                true
            }
            Pending::GreetCollapse if self.mode == Mode::Welcome => {
                self.mode = Mode::Petit;
                true
            }
            _ => false,
        }
    }

    // ---- timers ---------------------------------------------------------------

    fn set(&mut self, mode: Mode, _now: Instant) {
        self.mode = mode;
    }

    fn is_pending(&self, what: Pending) -> bool {
        matches!(self.pending, Some((pending, _)) if pending == what)
    }

    fn schedule(&mut self, what: Pending, seconds: f64, now: Instant) {
        let millis = (seconds.max(0.0) * 1000.0) as u64;
        self.pending = Some((what, now + Duration::from_millis(millis)));
    }

    fn clear(&mut self, what: Pending) {
        if self.is_pending(what) {
            self.pending = None;
        }
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Instant {
        // One fixed instant, then offsets: no sleeping, and no clock reads that
        // could differ between two calls.
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(Instant::now) + Duration::from_millis(ms)
    }

    #[test]
    fn a_launch_opens_the_welcome_and_rests_before_shrinking() {
        let mut fsm = CompanionFsm::new();
        assert_eq!(fsm.mode(), Mode::Hidden);
        fsm.launch(at(0));
        assert_eq!(fsm.mode(), Mode::Welcome);
        // The greeting has not finished, so nothing is scheduled.
        fsm.poll(at(10_000));
        assert_eq!(fsm.mode(), Mode::Welcome);
        fsm.greet_complete(at(4_600));
        assert_eq!(fsm.next_deadline(), Some(at(4_600) + Duration::from_millis(1_400)));
        assert!(!fsm.poll(at(5_900)), "still resting");
        assert!(fsm.poll(at(6_000)));
        assert_eq!(fsm.mode(), Mode::Petit);
    }

    #[test]
    fn leaving_the_welcome_early_does_not_cut_the_greeting_short() {
        let mut fsm = CompanionFsm::new();
        fsm.launch(at(0));
        fsm.mouse_left(at(500));
        assert_eq!(fsm.mode(), Mode::Welcome);
        assert_eq!(fsm.next_deadline(), None, "not greeted yet, so nothing is scheduled");
        fsm.greet_complete(at(4_600));
        fsm.poll(at(6_000));
        assert_eq!(fsm.mode(), Mode::Petit, "it rests, then shrinks");
    }

    #[test]
    fn hovering_the_greeting_keeps_it_for_ten_seconds() {
        let mut fsm = CompanionFsm::new();
        fsm.launch(at(0));
        fsm.mouse_entered(at(100));
        assert_eq!(fsm.next_deadline(), Some(at(100) + Duration::from_millis(10_000)));
        fsm.greet_complete(at(4_600));
        assert_eq!(
            fsm.next_deadline(),
            Some(at(100) + Duration::from_millis(10_000)),
            "a finished greeting does not override a running hover timer"
        );
    }

    #[test]
    fn a_click_expands_and_leaving_collapses_after_fifteen_seconds() {
        let mut fsm = CompanionFsm::new();
        fsm.reveal(at(0));
        assert_eq!(fsm.mode(), Mode::Petit);
        fsm.click(at(100));
        assert_eq!(fsm.mode(), Mode::Home);
        fsm.mouse_left(at(200));
        assert!(!fsm.poll(at(14_000)));
        assert!(fsm.poll(at(15_200)));
        assert_eq!(fsm.mode(), Mode::Petit);
    }

    #[test]
    fn a_pinned_alert_stays_open() {
        let mut fsm = CompanionFsm::new();
        fsm.force_home(at(0));
        assert_eq!(fsm.mode(), Mode::Home);
        fsm.mouse_left(at(100));
        assert!(!fsm.poll(at(60_000)), "pinned: the collapse is refused");
        assert_eq!(fsm.mode(), Mode::Home);
        fsm.force_petit(false, at(60_100));
        assert_eq!(fsm.mode(), Mode::Home, "an unforced close respects the pin");
        fsm.force_petit(true, at(60_200));
        assert_eq!(fsm.mode(), Mode::Petit);
        assert!(!fsm.pinned);
    }

    #[test]
    fn the_compact_bar_never_hides_itself() {
        // The Bitbots adapter's own change: a freestanding overlay must not
        // vanish into an unknown desktop position.
        let mut fsm = CompanionFsm::new();
        fsm.reveal(at(0));
        fsm.mouse_left(at(100));
        assert_eq!(fsm.next_deadline(), None);
        assert!(!fsm.poll(at(600_000)));
        assert_eq!(fsm.mode(), Mode::Petit);
    }

    #[test]
    fn the_shortcut_toggles_between_hidden_and_the_bar() {
        let mut fsm = CompanionFsm::new();
        fsm.reveal(at(0));
        fsm.toggle(at(100));
        assert_eq!(fsm.mode(), Mode::Hidden);
        fsm.toggle(at(200));
        assert_eq!(fsm.mode(), Mode::Petit);
        fsm.click(at(300));
        fsm.toggle(at(400));
        assert_eq!(fsm.mode(), Mode::Petit, "an expanded companion closes to the bar");
    }

    #[test]
    fn a_hidden_companion_is_off_screen_and_a_petit_one_is_not() {
        assert!(!Mode::Hidden.visible());
        assert!(Mode::Petit.visible() && Mode::Home.visible() && Mode::Welcome.visible());
        assert_eq!(Mode::Home.key(), "home");
    }
}
