//! TextInjector (FR-05) - synthesizes Ctrl+V via SendInput, tagged with a
//! magic dwExtraInfo marker so our own WH_KEYBOARD_LL hook ignores it,
//! and waits for residual physical modifiers before pasting.
//!
//! Full FR-05 sequence - wait for modifier release, snapshot the user
//! clipboard, set the transcription as CF_UNICODETEXT, synthesize the
//! paste, wait PASTE_SETTLE_MS, restore the previous clipboard.

use std::mem::size_of;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::clipboard;

use tracing::{debug, info, warn};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY,
};

/// Magic dwExtraInfo tag attached to every INPUT this module synthesizes.
/// The WH_KEYBOARD_LL hook (hotkey module) drops events carrying this
/// marker - otherwise our own synthetic Ctrl+V would feed back into the
/// combo tracker and could cancel an active combo (V counts as a third
/// key) or re-trigger one.
pub const INJECT_MARKER: usize = 0x1A9C7B0E;

// Virtual key codes used by the injector (winuser.h values).
const VK_CONTROL: u16 = 0x11;
const VK_V: u16 = 0x56;

// Modifiers polled by wait_for_modifiers_released. Ctrl and Win are the
// combo keys (FR-01); Shift and Alt are polled too for robustness - a
// paste with ANY modifier still physically held is corrupted (Ctrl+Win+V
// opens the Windows clipboard history popup; Ctrl+Shift+V or Ctrl+Alt+V
// trigger app-specific shortcuts instead of a plain paste).
const VK_SHIFT: i32 = 0x10;
const VK_MENU: i32 = 0x12;
const VK_LWIN: i32 = 0x5B;
const VK_RWIN: i32 = 0x5C;
const VK_LCONTROL: i32 = 0xA2;
const VK_RCONTROL: i32 = 0xA3;

/// Poll cadence while waiting for modifier release.
const RELEASE_POLL_INTERVAL: Duration = Duration::from_millis(15);

/// Errors surfaced by the text injector.
#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    /// SendInput injected fewer key events than requested (sent, expected).
    #[error("SendInput injected {0} of {1} key events")]
    SendInputFailed(u32, u32),
    /// A clipboard step (snapshot or set_text) of the sequence failed.
    #[error("clipboard step failed during injection - {0}")]
    Clipboard(#[from] clipboard::ClipboardError),
}

/// True when a low-level keyboard event carries our own injection marker
/// and must NOT be fed to the combo tracker (anti-feedback, FR-05).
/// Pure so the discard rule is unit-testable.
pub fn is_own_injection(extra_info: usize) -> bool {
    extra_info == INJECT_MARKER
}

/// The Ctrl+V chord as (virtual key, is_key_up) pairs in emission order.
/// Pure so the ordering is unit-testable.
fn ctrl_v_key_sequence() -> [(u16, bool); 4] {
    [
        (VK_CONTROL, false),
        (VK_V, false),
        (VK_V, true),
        (VK_CONTROL, true),
    ]
}

/// Builds one keyboard INPUT event tagged with INJECT_MARKER.
fn key_input(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: INJECT_MARKER,
            },
        },
    }
}

/// Best-effort compensation for a partially delivered chord - emits
/// marked key-up events for Ctrl and V so no synthetic key stays pressed
/// at system level. Failures are only logged; the caller is already on
/// an error path and the original error is the one worth propagating.
fn release_ctrl_v_best_effort() {
    let ups: Vec<INPUT> = [(VK_CONTROL, true), (VK_V, true)]
        .iter()
        .map(|&(vk, key_up)| key_input(vk, key_up))
        .collect();
    // SAFETY: ups is fully initialized and SendInput copies the slice.
    let sent = unsafe { SendInput(&ups, size_of::<INPUT>() as i32) };
    if sent as usize != ups.len() {
        warn!(
            sent,
            "compensatory Ctrl/V key-up injection incomplete - a synthetic modifier may remain pressed"
        );
    }
}

/// Synthesizes a Ctrl+V chord (Ctrl down, V down, V up, Ctrl up) in a
/// single SendInput call so no real keystroke can interleave. Every event
/// is tagged with INJECT_MARKER (see is_own_injection).
pub fn send_ctrl_v() -> Result<(), InjectError> {
    let inputs: Vec<INPUT> = ctrl_v_key_sequence()
        .iter()
        .map(|&(vk, key_up)| key_input(vk, key_up))
        .collect();
    // SAFETY: inputs is fully initialized and SendInput copies the slice.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        // A partial injection (0 < sent < 4) may have delivered the
        // key-downs but not the key-ups, leaving a SYNTHETIC Ctrl (and
        // possibly V) pressed system-wide - every subsequent real
        // keystroke would be silently modified by a Ctrl the user is
        // not holding. Compensate with best-effort marked key-ups
        // before surfacing the error. sent == 0 means nothing was
        // injected at all, so there is nothing to compensate.
        if sent > 0 {
            warn!(sent, "partial SendInput - emitting compensatory key-ups");
            release_ctrl_v_best_effort();
        }
        return Err(InjectError::SendInputFailed(sent, inputs.len() as u32));
    }
    Ok(())
}

/// True while any polled modifier is physically held down.
fn any_modifier_down() -> bool {
    const MODIFIERS: [i32; 6] = [
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LWIN,
        VK_RWIN,
        VK_SHIFT,
        VK_MENU,
    ];
    MODIFIERS.iter().any(|&vk| {
        // SAFETY: plain Win32 key-state query, no pointers involved.
        (unsafe { GetAsyncKeyState(vk) } as u16) & 0x8000 != 0
    })
}

/// Polls every 15 ms until Ctrl, Win, Shift and Alt are all physically
/// released or the timeout expires. Returns true when the keys ended up
/// free, false when the timeout expired with something still held.
///
/// Rationale - injection is triggered by RELEASING the Ctrl+Win combo, so
/// at that instant those keys are often still on their way up. If Win were
/// still down, our synthetic Ctrl+V would become Ctrl+Win+V, which opens
/// the Windows clipboard history popup instead of pasting; a lingering
/// Shift or Alt corrupts the paste the same way.
pub fn wait_for_modifiers_released(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if !any_modifier_down() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(RELEASE_POLL_INTERVAL);
    }
}

/// Milliseconds between the synthetic Ctrl+V and the clipboard restore.
/// Pasting is asynchronous - the target app reads the clipboard when it
/// processes the keystrokes - so restoring too early would paste the OLD
/// clipboard. Single system-wide settle value, referenced by the spec.
pub const PASTE_SETTLE_MS: u64 = 300;

/// How long inject() waits for residual physical modifiers to go up.
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_secs(2);

/// Serializes inject() calls. FRB dispatches API calls on a worker pool,
/// so two overlapping inject() invocations are possible; the sequence
/// mutates process-global and system-global state (clipboard contents,
/// synthetic input) and two interleaved runs would corrupt each other
/// (one run's restore erasing the other run's set_text, for example).
static INJECT_LOCK: Mutex<()> = Mutex::new(());

/// Outcome of a successful injection - in both cases the paste happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionOutcome {
    /// Text pasted and the previous clipboard contents were restored.
    Pasted,
    /// Text pasted but restoring the previous clipboard failed. Not
    /// fatal - the transcription reached the target app.
    PastedRestoreFailed,
}

/// Maps the clipboard-restore result onto the public outcome. Pure so
/// the mapping is unit-testable.
fn outcome_after_restore(restore_succeeded: bool) -> InjectionOutcome {
    if restore_succeeded {
        InjectionOutcome::Pasted
    } else {
        InjectionOutcome::PastedRestoreFailed
    }
}

/// Best-effort restore of the pre-injection snapshot on an error path.
/// FR-06 - the content the user had on the clipboard must not be lost;
/// without this, an error after set_text would propagate immediately and
/// leave the user clipboard holding our transcription (or nothing at
/// all). A failure here is only logged: the original error that put us
/// on this path is the one the caller must see.
fn restore_on_error(prev: &clipboard::ClipboardSnapshot) {
    if let Err(e) = clipboard::restore(prev) {
        warn!("clipboard restore on error path also failed - {e}");
    }
}

/// A successful injection plus the moment the paste was dispatched.
///
/// `to_paste` is the elapsed time from the start of [`inject_timed`]
/// (its own entry, i.e. the beginning of the modifier-release wait +
/// snapshot + set_text + send_ctrl_v) up to the instant `send_ctrl_v`
/// returned Ok - the "paste-landed" moment for NFR-01. It deliberately
/// EXCLUDES the post-paste housekeeping the user never waits on:
/// `PASTE_SETTLE_MS` and the clipboard restore. The `outcome` still
/// reflects the full sequence (settle + restore), which run unchanged.
#[derive(Debug, Clone, Copy)]
pub struct TimedInjection {
    /// Paste result after the full sequence (settle + restore).
    pub outcome: InjectionOutcome,
    /// inject_timed() entry -> Ctrl+V dispatched (paste-landed).
    pub to_paste: Duration,
}

/// Runs the full FR-05 injection sequence - wait for modifier release,
/// snapshot the clipboard, place the transcription on it, synthesize
/// Ctrl+V, wait PASTE_SETTLE_MS for the target app to consume the paste,
/// then restore the previous clipboard.
///
/// A restore failure is reported as PastedRestoreFailed, not as an
/// error, because the paste itself already happened. If a step fails
/// AFTER the snapshot, the snapshot is restored best-effort before the
/// error propagates (FR-06 - see restore_on_error).
///
/// Not reentrant by design: INJECT_LOCK serializes concurrent callers.
///
/// NFR-12 - the transcribed text is NEVER logged; only its length is.
pub fn inject(text: &str) -> Result<InjectionOutcome, InjectError> {
    inject_timed(text).map(|timed| timed.outcome)
}

/// Same sequence as [`inject`] but also reports the paste-landed moment
/// (see [`TimedInjection::to_paste`]). This is the NFR-01 measurement
/// path: instrumentation ONLY - the production behavior is identical
/// (settle + restore run exactly as in [`inject`]). Callers that do not
/// need the timing use [`inject`], which discards it.
pub fn inject_timed(text: &str) -> Result<TimedInjection, InjectError> {
    // Anchor for the paste-landed duration: the very start of the
    // sequence, so `to_paste` folds in the modifier-release wait too
    // (part of what stands between keyup and the visible paste).
    let started = Instant::now();
    // Poison recovery - a panic in a previous inject() leaves no state
    // behind the lock worth invalidating (the clipboard guard closes
    // itself on unwind), so the next caller can proceed safely.
    let _serial = INJECT_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    info!(len = text.len(), "InjectStart");
    if !wait_for_modifiers_released(MODIFIER_RELEASE_TIMEOUT) {
        warn!("physical modifiers still held after release timeout; pasting anyway");
    }
    let prev = clipboard::snapshot()?;
    debug!(formats = prev.formats().len(), "ClipboardSnapshotTaken");
    // From here on the user clipboard is at risk: any failure must put
    // the snapshot back before propagating (FR-06).
    if let Err(e) = clipboard::set_text(text) {
        restore_on_error(&prev);
        return Err(e.into());
    }
    if let Err(e) = send_ctrl_v() {
        restore_on_error(&prev);
        return Err(e);
    }
    // Paste-landed: Ctrl+V has been dispatched to the target app. This
    // is the NFR-01 stop mark - measured BEFORE the settle sleep and
    // the clipboard restore, which are post-paste housekeeping.
    let to_paste = started.elapsed();
    debug!("CtrlVSent");
    thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));
    let restored = match clipboard::restore(&prev) {
        Ok(()) => true,
        Err(e) => {
            warn!("clipboard restore failed after paste - {e}");
            false
        }
    };
    let outcome = outcome_after_restore(restored);
    info!(?outcome, "InjectDone");
    Ok(TimedInjection { outcome, to_paste })
}

/// In-memory backup of the last transcribed text (FR-05 fallback).
/// There is no reliable way to detect that a target app blocked the
/// paste (pasting does not mutate the clipboard), so the last text is
/// always kept here; the orchestrator (phase 4) populates it and the UI
/// can hand it back to the user on demand. NFR-12 - it lives only in
/// process memory and is never logged.
static LAST_TRANSCRIPTION: Mutex<Option<String>> = Mutex::new(None);

/// Stores text as the last-transcription backup.
pub fn store_last_transcription(text: &str) {
    let mut slot = LAST_TRANSCRIPTION
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    *slot = Some(text.to_owned());
}

/// Returns a copy of the last stored transcription, if any.
pub fn last_transcription() -> Option<String> {
    LAST_TRANSCRIPTION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Drops the in-memory backup (NFR-12 hygiene - dictated text should not
/// outlive its usefulness). MUST be wired into the app shutdown path by
/// the orchestrator (shutdown hygiene);
/// nothing calls it yet in phase 3.
pub fn clear_last_transcription() {
    let mut slot = LAST_TRANSCRIPTION
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    *slot = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Task 2 pure logic (TDD) ----

    #[test]
    fn own_marker_is_recognized_as_own_injection() {
        assert!(is_own_injection(INJECT_MARKER));
    }

    #[test]
    fn foreign_extra_info_is_not_own_injection() {
        assert!(!is_own_injection(0));
        assert!(!is_own_injection(0xDEADBEEF));
        assert!(!is_own_injection(INJECT_MARKER + 1));
    }

    #[test]
    fn ctrl_v_sequence_is_a_well_formed_chord() {
        assert_eq!(
            ctrl_v_key_sequence(),
            [
                (VK_CONTROL, false),
                (VK_V, false),
                (VK_V, true),
                (VK_CONTROL, true),
            ]
        );
    }

    #[test]
    fn key_input_tags_marker_and_key_up_flag() {
        let down = key_input(VK_V, false);
        let up = key_input(VK_V, true);
        // SAFETY: both INPUTs were built as INPUT_KEYBOARD, so reading
        // the ki union variant is valid.
        unsafe {
            assert_eq!(down.Anonymous.ki.dwExtraInfo, INJECT_MARKER);
            assert_eq!(up.Anonymous.ki.dwExtraInfo, INJECT_MARKER);
            assert_eq!(down.Anonymous.ki.dwFlags, KEYBD_EVENT_FLAGS(0));
            assert_eq!(up.Anonymous.ki.dwFlags, KEYEVENTF_KEYUP);
            assert_eq!(down.Anonymous.ki.wVk, VIRTUAL_KEY(VK_V));
        }
    }

    // ---- Task 3 pure logic (TDD) ----

    #[test]
    fn restore_success_maps_to_pasted() {
        assert_eq!(outcome_after_restore(true), InjectionOutcome::Pasted);
    }

    #[test]
    fn restore_failure_maps_to_pasted_restore_failed() {
        assert_eq!(
            outcome_after_restore(false),
            InjectionOutcome::PastedRestoreFailed
        );
    }

    /// Serializes the tests below - they mutate the process-global
    /// LAST_TRANSCRIPTION and the default test runner is multi-threaded.
    static LAST_TX_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn last_transcription_roundtrip_and_overwrite() {
        let _serial = LAST_TX_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        store_last_transcription("first");
        assert_eq!(last_transcription().as_deref(), Some("first"));
        store_last_transcription("second");
        assert_eq!(last_transcription().as_deref(), Some("second"));
    }

    #[test]
    fn clear_last_transcription_empties_the_backup() {
        let _serial = LAST_TX_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        store_last_transcription("to be wiped");
        assert!(last_transcription().is_some());
        clear_last_transcription();
        assert_eq!(last_transcription(), None);
    }
}

#[cfg(test)]
mod integration_tests {
    //! Integration tests against a real, self-contained EDIT window and
    //! the real clipboard. They synthesize real key events, so run them
    //! focused and hands off the keyboard, one at a time, with
    //! cargo test --lib inject -- --ignored --nocapture --test-threads=1

    use super::*;
    use crate::clipboard;
    use std::sync::mpsc::channel;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow, GetMessageW,
        GetWindowThreadProcessId, PostThreadMessageW, SendMessageW, SetForegroundWindow,
        TranslateMessage, MSG, WINDOW_EX_STYLE, WM_GETTEXT, WM_QUIT, WS_POPUP, WS_VISIBLE,
    };

    const CF_UNICODETEXT: u32 = 13;

    /// Courtesy snapshot of the USER clipboard that restores itself on
    /// drop - so whatever the user had copied survives the test even
    /// when an assert panics halfway through.
    struct UserClipboardGuard(clipboard::ClipboardSnapshot);

    impl UserClipboardGuard {
        fn capture() -> Self {
            Self(clipboard::snapshot().expect("snapshot of user clipboard"))
        }
    }

    impl Drop for UserClipboardGuard {
        fn drop(&mut self) {
            if let Err(e) = clipboard::restore(&self.0) {
                eprintln!("courtesy restore of user clipboard failed - {e}");
            }
        }
    }

    /// Brings hwnd to the foreground and focuses it, retrying with the
    /// AttachThreadInput workaround. Windows denies SetForegroundWindow
    /// to background processes (a cargo test run usually is one), but
    /// attaching our input queue to the thread that owns the current
    /// foreground window lifts that restriction.
    fn acquire_focus(hwnd: HWND) -> bool {
        for _ in 0..10 {
            // SAFETY: plain Win32 calls; hwnd is owned by this thread.
            unsafe {
                let fg = GetForegroundWindow();
                let my_thread = GetCurrentThreadId();
                let fg_thread = if fg.0.is_null() {
                    0
                } else {
                    GetWindowThreadProcessId(fg, None)
                };
                let attached = fg_thread != 0
                    && fg_thread != my_thread
                    && AttachThreadInput(my_thread, fg_thread, true).as_bool();
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                if attached {
                    let _ = AttachThreadInput(my_thread, fg_thread, false);
                }
                if GetForegroundWindow() == hwnd {
                    return true;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// Self-contained EDIT window with its own message pump thread.
    /// WM_GETTEXT is sent from the test thread; SendMessage marshals the
    /// call to the pump thread synchronously.
    struct EditWindow {
        hwnd: isize,
        thread_id: u32,
        focused: bool,
        join: Option<thread::JoinHandle<()>>,
    }

    impl EditWindow {
        fn spawn() -> Self {
            let (tx, rx) = channel::<(isize, u32, bool)>();
            let join = thread::spawn(move || {
                // SAFETY: EDIT is a system window class (no registration
                // needed); a visible popup EDIT is a valid top-level
                // window that accepts keyboard focus and paste.
                let hwnd = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        w!("EDIT"),
                        // Empty title - for an EDIT control the window
                        // text IS its content, which must start empty.
                        w!(""),
                        WS_POPUP | WS_VISIBLE,
                        100,
                        100,
                        420,
                        60,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                .expect("CreateWindowExW(EDIT) failed");
                let focused = acquire_focus(hwnd);
                // SAFETY: plain Win32 call, no arguments.
                let thread_id = unsafe { GetCurrentThreadId() };
                tx.send((hwnd.0 as isize, thread_id, focused))
                    .expect("spawner is waiting for window info");
                let mut msg = MSG::default();
                // SAFETY: msg is a valid, writable MSG for every call.
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
                    // SAFETY: msg was filled by a successful GetMessageW.
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                // SAFETY: hwnd was created by this thread and is alive.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            });
            let (hwnd, thread_id, focused) = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("EDIT window thread did not report in time");
            Self {
                hwnd,
                thread_id,
                focused,
                join: Some(join),
            }
        }

        fn hwnd(&self) -> HWND {
            HWND(self.hwnd as *mut core::ffi::c_void)
        }

        /// Reads the EDIT content with WM_GETTEXT.
        fn text(&self) -> String {
            let mut buf = [0u16; 256];
            // SAFETY: buf outlives the synchronous SendMessageW call and
            // WM_GETTEXT writes at most wParam minus one chars plus null.
            let len = unsafe {
                SendMessageW(
                    self.hwnd(),
                    WM_GETTEXT,
                    Some(WPARAM(buf.len())),
                    Some(LPARAM(buf.as_mut_ptr() as isize)),
                )
            };
            String::from_utf16_lossy(&buf[..len.0 as usize])
        }
    }

    impl Drop for EditWindow {
        fn drop(&mut self) {
            // SAFETY: the pump thread owns a message queue while alive.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    /// Decodes the CF_UNICODETEXT entry of a fresh snapshot, if present.
    fn clipboard_text() -> Option<String> {
        let snap = clipboard::snapshot().expect("snapshot for readback");
        let bytes = snap
            .formats()
            .iter()
            .find(|(format, _)| *format == CF_UNICODETEXT)
            .map(|(_, bytes)| bytes.clone())?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    fn send_marked_key(vk: u16, key_up: bool) {
        let input = key_input(vk, key_up);
        // SAFETY: input is fully initialized and SendInput copies it.
        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        assert_eq!(sent, 1, "SendInput failed for marked vk {vk:#04X}");
    }

    #[test]
    #[ignore = "creates a real window and injects real input; run focused, --test-threads=1"]
    fn inject_types_into_own_edit_window() {
        // Guard, not a plain snapshot: restores the user clipboard even
        // if an assert below panics (courtesy restore via Drop).
        let _user_clipboard = UserClipboardGuard::capture();
        let window = EditWindow::spawn();
        assert!(
            window.focused,
            "could not bring the EDIT window to the foreground (Windows focus rules) - rerun with the test terminal focused"
        );

        clipboard::set_text("hola inyector").expect("set_text");
        send_ctrl_v().expect("send_ctrl_v");
        thread::sleep(Duration::from_millis(300));

        assert_eq!(window.text(), "hola inyector");
    }

    #[test]
    #[ignore = "injects real key events; run focused, --test-threads=1"]
    fn hook_ignores_marked_input() {
        use crate::hotkey::HotkeyMonitor;

        // WARNING - this test injects a REAL Win keydown and keyup. Even
        // though our hook ignores the marked events, Windows itself does
        // not: depending on timing the OS may interpret the sequence as
        // a Win tap and open the Start menu after the test. Harmless but
        // surprising; tolerable for a manually-run #[ignore] test.
        let (tx, rx) = channel();
        let handle = HotkeyMonitor::start(tx).expect("monitor should start");

        // Ctrl+Win WITH our marker - the hook must drop these before the
        // combo tracker, so no ComboPressed may be emitted.
        send_marked_key(VK_LCONTROL as u16, false);
        send_marked_key(VK_LWIN as u16, false);
        let got = rx.recv_timeout(Duration::from_millis(500));
        send_marked_key(VK_LWIN as u16, true);
        send_marked_key(VK_LCONTROL as u16, true);
        handle.stop();

        assert!(
            got.is_err(),
            "hook fed marked input to the tracker - got {got:?}"
        );
    }

    #[test]
    #[ignore = "creates a real window, injects real input and touches the real clipboard"]
    fn full_inject_sequence_preserves_clipboard() {
        // Guard, not a plain snapshot: restores the user clipboard even
        // if an assert below panics (courtesy restore via Drop).
        let _user_clipboard = UserClipboardGuard::capture();

        // Seed a known previous clipboard, then run the FULL sequence.
        clipboard::set_text("PREVIO").expect("set_text PREVIO");
        let window = EditWindow::spawn();
        assert!(
            window.focused,
            "could not bring the EDIT window to the foreground (Windows focus rules) - rerun with the test terminal focused"
        );

        let outcome = inject("texto dictado").expect("inject");

        assert_eq!(outcome, InjectionOutcome::Pasted);
        assert_eq!(window.text(), "texto dictado");
        assert_eq!(clipboard_text().as_deref(), Some("PREVIO"));
    }
}
