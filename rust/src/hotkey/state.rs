//! `ComboTracker`: pure state machine for the Ctrl+Win combo (FR-01).
//!
//! No Win32 calls, no threads, no I/O — it consumes key events
//! (virtual-key codes + up/down) and emits combo transitions. Time is
//! injected through a clock closure so hold durations are fully testable.
//!
//! Combo = (any Ctrl down) AND (any Win down). The low-level hook
//! delivers side-specific VKs (`VK_LCONTROL`/`VK_RCONTROL`,
//! `VK_LWIN`/`VK_RWIN`), never the generic `VK_CONTROL`.
//!
//! A third key pressed while the combo is held cancels it immediately
//! (`ReleaseReason::OtherKeyPressed`): the OS owns that shortcut
//! (e.g. Ctrl+Win+D). After a cancel, a new `ComboPressed` requires a
//! fresh key-down that re-satisfies the combo condition.

use std::time::{Duration, Instant};

/// Virtual-key code for the left Control key.
pub const VK_LCONTROL: u32 = 0xA2;
/// Virtual-key code for the right Control key.
pub const VK_RCONTROL: u32 = 0xA3;
/// Virtual-key code for the left Windows key.
pub const VK_LWIN: u32 = 0x5B;
/// Virtual-key code for the right Windows key.
pub const VK_RWIN: u32 = 0x5C;

/// A raw key transition as seen by the low-level keyboard hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerInput {
    /// Key went down (WM_KEYDOWN / WM_SYSKEYDOWN). Payload: virtual-key code.
    KeyDown(u32),
    /// Key went up (WM_KEYUP / WM_SYSKEYUP). Payload: virtual-key code.
    KeyUp(u32),
}

/// Why an active combo was released.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseReason {
    /// One of the combo keys (Ctrl or Win) was lifted.
    KeyLifted,
    /// A third key was pressed while the combo was held; the OS owns
    /// that shortcut, so dictation must cancel immediately (FR-01).
    OtherKeyPressed,
    /// The interactive session was locked while the combo was held.
    /// The tracker is force-reset so no combo state survives the lock
    /// (FR-01); the release is surfaced with this distinct reason so
    /// downstream consumers can tell a lock apart from a normal lift.
    SessionLocked,
}

/// Result of feeding one key event into the tracker.
///
/// The idle variant is named `Nothing` (not `None`) to avoid constant
/// confusion with `Option::None` at call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerOutput {
    /// The combo just became active (both Ctrl and Win are now down).
    ComboPressed,
    /// The combo just became inactive.
    ComboReleased {
        /// Time the combo was held, measured with the injected clock.
        hold: Duration,
        /// What ended the combo.
        reason: ReleaseReason,
    },
    /// No combo transition.
    Nothing,
}

/// Pure state machine tracking the Ctrl+Win combo.
///
/// Generic over a clock closure so tests can control time; production
/// code uses [`ComboTracker::new`], which reads `Instant::now`.
pub struct ComboTracker<C = fn() -> Instant>
where
    C: FnMut() -> Instant,
{
    lctrl_down: bool,
    rctrl_down: bool,
    lwin_down: bool,
    rwin_down: bool,
    /// `Some(t)` while the combo is active; `t` = activation instant.
    active_since: Option<Instant>,
    clock: C,
}

impl ComboTracker<fn() -> Instant> {
    /// Tracker with the real monotonic clock.
    pub fn new() -> Self {
        Self::with_clock(Instant::now)
    }
}

impl Default for ComboTracker<fn() -> Instant> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> ComboTracker<C>
where
    C: FnMut() -> Instant,
{
    /// Tracker with an injected clock (tests).
    pub fn with_clock(clock: C) -> Self {
        Self {
            lctrl_down: false,
            rctrl_down: false,
            lwin_down: false,
            rwin_down: false,
            active_since: None,
            clock,
        }
    }

    /// Feed one key event; returns the combo transition it caused, if any.
    pub fn feed(&mut self, input: TrackerInput) -> TrackerOutput {
        match input {
            TrackerInput::KeyDown(vk) => self.on_key_down(vk),
            TrackerInput::KeyUp(vk) => self.on_key_up(vk),
        }
    }

    /// Force the tracker back to the idle state, discarding all held-key
    /// flags. Used when the OS pulls the rug out from under us — session
    /// lock (Win+L), where the key-up events for a held combo may never
    /// reach the hook.
    ///
    /// If the combo was active, returns [`TrackerOutput::ComboReleased`]
    /// with [`ReleaseReason::SessionLocked`] (measured hold) so the caller
    /// can close the dictation cleanly before announcing the lock;
    /// otherwise returns [`TrackerOutput::Nothing`].
    pub fn reset(&mut self) -> TrackerOutput {
        let out = match self.active_since.take() {
            Some(since) => TrackerOutput::ComboReleased {
                hold: self.hold_since(since),
                reason: ReleaseReason::SessionLocked,
            },
            None => TrackerOutput::Nothing,
        };
        self.lctrl_down = false;
        self.rctrl_down = false;
        self.lwin_down = false;
        self.rwin_down = false;
        out
    }

    fn on_key_down(&mut self, vk: u32) -> TrackerOutput {
        if let Some(was_down) = self.combo_flag(vk) {
            if was_down {
                // OS autorepeat of a held key: no transition.
                return TrackerOutput::Nothing;
            }
            self.set_combo_flag(vk, true);
            if self.active_since.is_none() && self.combo_keys_held() {
                self.active_since = Some((self.clock)());
                return TrackerOutput::ComboPressed;
            }
            return TrackerOutput::Nothing;
        }
        // Non-combo key. If the combo is active, the OS owns this
        // shortcut (e.g. Ctrl+Win+D): cancel immediately (FR-01).
        match self.active_since.take() {
            Some(since) => TrackerOutput::ComboReleased {
                hold: self.hold_since(since),
                reason: ReleaseReason::OtherKeyPressed,
            },
            None => TrackerOutput::Nothing,
        }
    }

    fn on_key_up(&mut self, vk: u32) -> TrackerOutput {
        if let Some(was_down) = self.combo_flag(vk) {
            if !was_down {
                // KeyUp of a key we never saw go down (e.g. pressed
                // before the hook was installed): no transition.
                return TrackerOutput::Nothing;
            }
            self.set_combo_flag(vk, false);
            if !self.combo_keys_held() {
                if let Some(since) = self.active_since.take() {
                    return TrackerOutput::ComboReleased {
                        hold: self.hold_since(since),
                        reason: ReleaseReason::KeyLifted,
                    };
                }
            }
        }
        // KeyUp of a non-combo key never affects the combo.
        TrackerOutput::Nothing
    }

    /// `Some(is_down)` for the four combo VKs, `None` for any other key.
    fn combo_flag(&self, vk: u32) -> Option<bool> {
        match vk {
            VK_LCONTROL => Some(self.lctrl_down),
            VK_RCONTROL => Some(self.rctrl_down),
            VK_LWIN => Some(self.lwin_down),
            VK_RWIN => Some(self.rwin_down),
            _ => None,
        }
    }

    fn set_combo_flag(&mut self, vk: u32, down: bool) {
        match vk {
            VK_LCONTROL => self.lctrl_down = down,
            VK_RCONTROL => self.rctrl_down = down,
            VK_LWIN => self.lwin_down = down,
            VK_RWIN => self.rwin_down = down,
            _ => {}
        }
    }

    /// Combo condition: any Ctrl AND any Win currently down.
    fn combo_keys_held(&self) -> bool {
        (self.lctrl_down || self.rctrl_down) && (self.lwin_down || self.rwin_down)
    }

    /// Hold duration measured with the injected clock. Saturating: a
    /// non-monotonic mock clock can never produce a panic here.
    fn hold_since(&mut self, since: Instant) -> Duration {
        (self.clock)().saturating_duration_since(since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A controllable clock: `handle.set(...)` moves time; the closure
    /// returned reads the current value.
    fn mock_clock() -> (Rc<Cell<Instant>>, impl FnMut() -> Instant) {
        let now = Rc::new(Cell::new(Instant::now()));
        let reader = Rc::clone(&now);
        (now, move || reader.get())
    }

    fn advance(clock: &Rc<Cell<Instant>>, ms: u64) {
        clock.set(clock.get() + Duration::from_millis(ms));
    }

    const VK_F13: u32 = 0x7C;
    const VK_A: u32 = 0x41;

    #[test]
    fn lctrl_then_lwin_emits_combo_pressed() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::ComboPressed);
    }

    #[test]
    fn lwin_then_lctrl_emits_combo_pressed() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LCONTROL)), TrackerOutput::ComboPressed);
    }

    #[test]
    fn right_side_variants_emit_combo_pressed() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_RCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_RWIN)), TrackerOutput::ComboPressed);
    }

    #[test]
    fn releasing_ctrl_emits_key_lifted_with_measured_hold() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        advance(&clock, 300);
        assert_eq!(
            t.feed(TrackerInput::KeyUp(VK_LCONTROL)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(300),
                reason: ReleaseReason::KeyLifted,
            }
        );
    }

    #[test]
    fn releasing_win_emits_key_lifted_with_measured_hold() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        advance(&clock, 450);
        assert_eq!(
            t.feed(TrackerInput::KeyUp(VK_LWIN)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(450),
                reason: ReleaseReason::KeyLifted,
            }
        );
    }

    #[test]
    fn third_key_while_combo_active_emits_other_key_pressed() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        advance(&clock, 120);
        assert_eq!(
            t.feed(TrackerInput::KeyDown(VK_F13)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(120),
                reason: ReleaseReason::OtherKeyPressed,
            }
        );
        // Combo is gone: lifting the combo keys afterwards is a no-op.
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LWIN)), TrackerOutput::Nothing);
    }

    #[test]
    fn foreign_key_without_combo_emits_nothing() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_A)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_A)), TrackerOutput::Nothing);
        // Even with one combo key half-pressed.
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_A)), TrackerOutput::Nothing);
    }

    #[test]
    fn re_press_after_release_emits_new_combo_pressed() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        advance(&clock, 100);
        t.feed(TrackerInput::KeyUp(VK_LWIN));
        // Ctrl is still down; pressing Win again re-forms the combo.
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::ComboPressed);
        advance(&clock, 200);
        assert_eq!(
            t.feed(TrackerInput::KeyUp(VK_LCONTROL)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(200),
                reason: ReleaseReason::KeyLifted,
            }
        );
    }

    #[test]
    fn autorepeat_keydown_of_held_key_emits_nothing() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        // Autorepeat before the combo forms.
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LCONTROL)), TrackerOutput::Nothing);
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        // Autorepeat of both combo keys while the combo is active must
        // not re-emit ComboPressed nor cancel the combo.
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::Nothing);
    }

    #[test]
    fn keyup_of_key_never_down_emits_nothing() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LWIN)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_F13)), TrackerOutput::Nothing);
    }

    #[test]
    fn keyup_of_foreign_key_during_active_combo_does_not_release() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        // A KeyUp of an unrelated key (e.g. released after the combo
        // formed) must not cancel dictation.
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_A)), TrackerOutput::Nothing);
        advance(&clock, 500);
        // Combo still alive and hold still measured from activation.
        assert_eq!(
            t.feed(TrackerInput::KeyUp(VK_LWIN)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(500),
                reason: ReleaseReason::KeyLifted,
            }
        );
    }

    #[test]
    fn reset_while_combo_active_emits_session_locked_release() {
        let (clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        t.feed(TrackerInput::KeyDown(VK_LWIN));
        advance(&clock, 250);
        assert_eq!(
            t.reset(),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(250),
                reason: ReleaseReason::SessionLocked,
            }
        );
        // After a reset, the stale key-up events (which the OS may or may
        // not deliver post-lock) must NOT emit a second release.
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LCONTROL)), TrackerOutput::Nothing);
        assert_eq!(t.feed(TrackerInput::KeyUp(VK_LWIN)), TrackerOutput::Nothing);
        // And a brand-new cycle works cleanly afterwards.
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::ComboPressed);
        advance(&clock, 80);
        assert_eq!(
            t.feed(TrackerInput::KeyUp(VK_LWIN)),
            TrackerOutput::ComboReleased {
                hold: Duration::from_millis(80),
                reason: ReleaseReason::KeyLifted,
            }
        );
    }

    #[test]
    fn reset_while_idle_emits_nothing() {
        let (_clock, reader) = mock_clock();
        let mut t = ComboTracker::with_clock(reader);
        assert_eq!(t.reset(), TrackerOutput::Nothing);
        // Half-pressed (combo never formed) also resets to Nothing.
        t.feed(TrackerInput::KeyDown(VK_LCONTROL));
        assert_eq!(t.reset(), TrackerOutput::Nothing);
        // The half-press was cleared: a lone Win afterwards is not a combo.
        assert_eq!(t.feed(TrackerInput::KeyDown(VK_LWIN)), TrackerOutput::Nothing);
    }
}
