//! Global hotkey detection (FR-01).
//!
//! `HotkeyMonitor` runs a dedicated thread that installs a low-level
//! keyboard hook (`SetWindowsHookEx(WH_KEYBOARD_LL)`), runs its own
//! message pump, and feeds raw key events into the pure [`state::ComboTracker`]
//! state machine. Combo transitions (Ctrl+Win pressed/released) are
//! published to an event channel consumed by the dictation orchestrator
//! and, via FRB streams, by the Flutter UI.
//!
//! The same thread also owns a hidden top-level window registered for
//! session notifications (`WTSRegisterSessionNotification`) so we detect
//! lock/unlock (Win+L) and force-reset the combo — a held combo whose
//! key-up events never arrive after a lock must not leave dictation
//! stuck open (FR-01).
//!
//! # Design notes
//!
//! - **One monitor per process.** The Win32 hook callback is a plain C
//!   function pointer with no context parameter, so the callback state
//!   must live in a global. A process-wide `Mutex<bool>` guard rejects a
//!   second concurrent monitor (a `OnceLock` would be write-once and
//!   forbid the stop/start cycle that FR-11 pause/resume needs).
//! - **Callback state is a `thread_local`.** `WH_KEYBOARD_LL` callbacks
//!   AND the session window's `WndProc` both run on the thread that owns
//!   the message pump, never on other threads. A `thread_local`
//!   therefore gives both lock-free access to the `ComboTracker` and the
//!   event `Sender`, keeping the hook callback well under the ~1 ms
//!   budget: the only cross-thread operation is the wait-free
//!   `mpsc::Sender::send`.
//! - **Keys are never consumed.** The callback always returns
//!   `CallNextHookEx`: inputVoice observes the combo but the OS keeps
//!   processing every key normally.
//! - **Injected input is accepted, except our own.** `SendInput` events
//!   pass through the LL hook (with `LLKHF_INJECTED` set) and are treated
//!   like real keystrokes - required by the synthetic integration tests
//!   below and an accepted risk for a personal, local-only app. The one
//!   exception is input tagged with [`crate::inject::INJECT_MARKER`] (our
//!   own synthetic Ctrl+V), which is dropped before the combo tracker.
//! - **A NORMAL hidden window, not `HWND_MESSAGE`.** Message-only
//!   windows are excluded from window enumeration and there are field
//!   reports of them not receiving `WM_WTSSESSION_CHANGE`; a top-level
//!   window that is simply never shown is the reliable choice.
//!
//! # Manual verification (deferred, FR-01.AC-6)
//!
//! Real session lock/unlock cannot be driven from a unit or synthetic
//! test — there is no supported API to lock the workstation and observe
//! the hook in-process deterministically. The tracker-reset logic is
//! unit-tested in [`state`]; the wiring (SESSION_LOCKED / SESSION_UNLOCKED
//! logs + combo reset) is validated manually with Win+L at phase close.

pub mod state;

use std::cell::RefCell;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

use tracing::{error, info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostThreadMessageW, RegisterClassW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    CW_USEDEFAULT, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WM_KEYDOWN,
    WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
    WS_OVERLAPPEDWINDOW,
};

pub use state::ReleaseReason;
use state::{ComboTracker, TrackerInput, TrackerOutput};

/// Window class + title for the hidden session-notification window.
const SESSION_CLASS_NAME: PCWSTR = w!("InputVoiceSessionWindow");
const SESSION_WINDOW_TITLE: PCWSTR = w!("inputVoice session sink");

/// Events published by the hotkey monitor thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// The Ctrl+Win combo just became active.
    ComboPressed,
    /// The Ctrl+Win combo just ended.
    ComboReleased {
        /// How long the combo was held, in milliseconds.
        hold_ms: u64,
        /// What ended the combo.
        reason: ReleaseReason,
    },
    /// The interactive session was locked (e.g. Win+L). If a combo was
    /// active it is released first (with `ReleaseReason::SessionLocked`).
    SessionLocked,
    /// The interactive session was unlocked.
    SessionUnlocked,
}

/// Errors starting or running the hotkey monitor.
#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    /// A monitor is already running in this process (one hook maximum).
    #[error("a hotkey monitor is already running in this process")]
    AlreadyRunning,
    /// `SetWindowsHookEx(WH_KEYBOARD_LL)` failed; payload = HRESULT.
    #[error("SetWindowsHookEx(WH_KEYBOARD_LL) failed (HRESULT {0:#010X})")]
    HookInstallFailed(i32),
    /// The monitor thread could not be spawned or died during init.
    #[error("hotkey monitor thread failed to start: {0}")]
    ThreadStartFailed(String),
}

/// Per-hook-thread state consumed by the keyboard hook callback and the
/// session window `WndProc`.
struct HookThreadState {
    tracker: ComboTracker,
    tx: Sender<HotkeyEvent>,
}

impl HookThreadState {
    /// Map a tracker transition to a public event and publish it.
    fn publish_tracker_output(&mut self, out: TrackerOutput) {
        match out {
            TrackerOutput::ComboPressed => self.emit(HotkeyEvent::ComboPressed),
            TrackerOutput::ComboReleased { hold, reason } => self.emit(HotkeyEvent::ComboReleased {
                hold_ms: hold.as_millis() as u64,
                reason,
            }),
            TrackerOutput::Nothing => {}
        }
    }

    fn emit(&mut self, event: HotkeyEvent) {
        // A closed receiver just means nobody is listening anymore; the
        // monitor keeps running until it is stopped explicitly.
        let _ = self.tx.send(event);
    }
}

thread_local! {
    /// Hook-callback + WndProc context. Only ever populated on the
    /// monitor thread; see the module docs for why this is a
    /// thread_local and not a process-wide static.
    static HOOK_STATE: RefCell<Option<HookThreadState>> = const { RefCell::new(None) };
}

/// Runs `f` with mutable access to the monitor-thread state, if it is
/// initialized. A no-op if called off-thread or before init. Never
/// panics: a hook callback / WndProc must not unwind into Win32.
fn with_hook_state<F: FnOnce(&mut HookThreadState)>(f: F) {
    HOOK_STATE.with(|cell| {
        if let Ok(mut borrow) = cell.try_borrow_mut() {
            if let Some(state) = borrow.as_mut() {
                f(state);
            }
        }
    });
}

/// Process-wide "a monitor is running" flag. Intentionally a `Mutex`
/// (re-writable) and not a `OnceLock`: FR-11 needs stop/start cycles.
static MONITOR_ACTIVE: Mutex<bool> = Mutex::new(false);

fn lock_active() -> std::sync::MutexGuard<'static, bool> {
    // A poisoned lock only means a thread panicked while holding it;
    // the boolean inside is still meaningful.
    MONITOR_ACTIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Clears the process-wide active flag when the monitor thread exits
/// through any path (normal stop, init failure, panic).
struct ActiveFlagGuard;

impl Drop for ActiveFlagGuard {
    fn drop(&mut self) {
        *lock_active() = false;
    }
}

/// Global Ctrl+Win hotkey monitor (FR-01). See module docs.
pub struct HotkeyMonitor;

impl HotkeyMonitor {
    /// Start the monitor thread; combo transitions are sent to `tx`.
    ///
    /// Fails with [`HotkeyError::AlreadyRunning`] if another monitor is
    /// active in this process, or [`HotkeyError::HookInstallFailed`] if
    /// the OS rejects the low-level hook.
    pub fn start(tx: Sender<HotkeyEvent>) -> Result<HotkeyMonitorHandle, HotkeyError> {
        {
            let mut active = lock_active();
            if *active {
                return Err(HotkeyError::AlreadyRunning);
            }
            *active = true;
        }

        // The thread reports its init outcome (thread id, or error)
        // through this one-shot channel before entering the pump.
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<u32, HotkeyError>>();
        let join = match std::thread::Builder::new()
            .name("hotkey-monitor".into())
            .spawn(move || hook_thread_main(tx, init_tx))
        {
            Ok(join) => join,
            Err(e) => {
                *lock_active() = false;
                return Err(HotkeyError::ThreadStartFailed(e.to_string()));
            }
        };

        match init_rx.recv() {
            Ok(Ok(thread_id)) => Ok(HotkeyMonitorHandle {
                thread_id,
                join: Some(join),
            }),
            Ok(Err(e)) => {
                // Thread already cleaned up (ActiveFlagGuard) and exited.
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err(HotkeyError::ThreadStartFailed(
                    "monitor thread exited before reporting initialization".into(),
                ))
            }
        }
    }
}

/// Handle owning the monitor thread. Stops the monitor on `stop()` or
/// on drop.
pub struct HotkeyMonitorHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl HotkeyMonitorHandle {
    /// Stop the monitor: post `WM_QUIT` to the hook thread's message
    /// pump and join it. The hook is uninstalled by the thread itself.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        // SAFETY: plain Win32 call; the target thread owns a message
        // queue (it created the hook before start() returned), so
        // posting WM_QUIT is always valid while it is alive.
        if let Err(e) = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        {
            warn!("PostThreadMessage(WM_QUIT) to hotkey thread failed: {e}");
        }
        if join.join().is_err() {
            error!("hotkey monitor thread panicked");
        }
    }
}

impl Drop for HotkeyMonitorHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Body of the dedicated monitor thread: install hook, set up the
/// session-notification window, pump messages, tear everything down.
fn hook_thread_main(tx: Sender<HotkeyEvent>, init_tx: Sender<Result<u32, HotkeyError>>) {
    let _active_guard = ActiveFlagGuard;

    // Populate the callback context BEFORE installing the hook so the
    // callback can never observe a half-initialized state.
    HOOK_STATE.with(|cell| {
        *cell.borrow_mut() = Some(HookThreadState {
            tracker: ComboTracker::new(),
            tx,
        });
    });

    // SAFETY: plain Win32 call; a null module handle is never returned
    // for the current process.
    let module = unsafe { GetModuleHandleW(None) };
    let hinstance: HINSTANCE = match module {
        Ok(m) => m.into(),
        Err(e) => {
            let _ = init_tx.send(Err(HotkeyError::HookInstallFailed(e.code().0)));
            return;
        }
    };

    // SAFETY: keyboard_hook_proc is 'static and matches HOOKPROC;
    // dwThreadId = 0 is required for WH_KEYBOARD_LL (global hook).
    let hook = match unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), Some(hinstance), 0)
    } {
        Ok(h) => h,
        Err(e) => {
            let _ = init_tx.send(Err(HotkeyError::HookInstallFailed(e.code().0)));
            return;
        }
    };
    // Literal "HookInstalled" is grep-able evidence for FR-11.
    info!(hook = ?hook, "HookInstalled");

    // Session lock/unlock detection is best-effort: if the window or the
    // WTS registration fails, the hotkey still works, so we log and go on.
    let session_window = SessionWindow::create(hinstance);

    // SAFETY: plain Win32 call, no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    if init_tx.send(Ok(thread_id)).is_err() {
        // start() vanished; nobody owns us. Tear down and bail out.
        if let Some(win) = session_window {
            win.destroy();
        }
        // SAFETY: `hook` is the live hook we just installed.
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
        info!(hook = ?hook, "HookUninstalled");
        return;
    }

    run_message_pump();

    if let Some(win) = session_window {
        win.destroy();
    }

    // SAFETY: `hook` is the live hook installed by this thread.
    if let Err(e) = unsafe { UnhookWindowsHookEx(hook) } {
        warn!("UnhookWindowsHookEx failed: {e}");
    }
    // Literal "HookUninstalled" is grep-able evidence for FR-11.
    info!(hook = ?hook, "HookUninstalled");
}

/// Blocking message pump; returns when `WM_QUIT` is received. Both the
/// `WH_KEYBOARD_LL` callback and the session window's `WndProc` are
/// invoked by the OS while this thread waits inside `GetMessageW`.
fn run_message_pump() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid, writable MSG for the whole call.
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match ret.0 {
            0 => break, // WM_QUIT
            -1 => {
                error!(
                    "GetMessageW failed in hotkey pump: {}",
                    windows::core::Error::from_thread()
                );
                break;
            }
            _ => {
                // SAFETY: `msg` was filled by a successful GetMessageW.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

/// `WH_KEYBOARD_LL` callback. Runs on the monitor thread while it sits
/// in `GetMessageW`. Budget: well under 1 ms — a couple of branches, a
/// pure state-machine step and one wait-free mpsc send. Never consumes
/// keys: always defers to `CallNextHookEx`.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: for WH_KEYBOARD_LL with code == HC_ACTION, lParam
        // points to a valid KBDLLHOOKSTRUCT for the duration of the
        // call (Win32 contract).
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        // Anti-feedback (FR-05) - drop our own synthetic input BEFORE it
        // reaches the combo tracker. The Ctrl+V that inject::send_ctrl_v
        // synthesizes travels through this global hook too; feeding it to
        // the tracker would cancel an active combo (V counts as a third
        // key) or could re-trigger one. Phase 2 synthetic tests inject
        // WITHOUT the marker, so they keep exercising the full path.
        if crate::inject::is_own_injection(kb.dwExtraInfo) {
            // SAFETY: forwarding the exact arguments we received is always valid.
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
        let input = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(TrackerInput::KeyDown(kb.vkCode)),
            WM_KEYUP | WM_SYSKEYUP => Some(TrackerInput::KeyUp(kb.vkCode)),
            _ => None,
        };
        if let Some(input) = input {
            with_hook_state(|hook_state| {
                let out = hook_state.tracker.feed(input);
                hook_state.publish_tracker_output(out);
            });
        }
    }
    // SAFETY: forwarding the exact arguments we received is always valid.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// One-time registration of the session window class for this process.
/// A `OnceLock` is correct here (unlike the monitor flag): the class
/// lives for the process lifetime and must be registered exactly once
/// even across stop/start cycles. Stores whether registration succeeded.
static SESSION_CLASS_REGISTERED: OnceLock<bool> = OnceLock::new();

fn ensure_session_class(hinstance: HINSTANCE) -> bool {
    *SESSION_CLASS_REGISTERED.get_or_init(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(session_wnd_proc),
            hInstance: hinstance,
            lpszClassName: SESSION_CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: `wc` is fully initialized and lives for the call;
        // `lpszClassName` is a 'static wide string.
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            warn!(
                "RegisterClassW for session window failed: {}; session lock detection disabled",
                windows::core::Error::from_thread()
            );
            false
        } else {
            true
        }
    })
}

/// Hidden top-level window that receives `WM_WTSSESSION_CHANGE`.
struct SessionWindow {
    hwnd: HWND,
}

impl SessionWindow {
    /// Create the hidden window and register it for session
    /// notifications. Returns `None` (logged) on any failure — session
    /// detection is best-effort and never blocks the hotkey.
    fn create(hinstance: HINSTANCE) -> Option<Self> {
        if !ensure_session_class(hinstance) {
            return None;
        }
        // Never shown (no ShowWindow), so it stays invisible. NORMAL
        // top-level window (not HWND_MESSAGE) so WM_WTSSESSION_CHANGE is
        // delivered reliably.
        // SAFETY: class is registered; all handles are valid/optional.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                SESSION_CLASS_NAME,
                SESSION_WINDOW_TITLE,
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(hinstance),
                None,
            )
        };
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(e) => {
                warn!("CreateWindowExW for session window failed: {e}; session lock detection disabled");
                return None;
            }
        };
        // SAFETY: `hwnd` is a valid window owned by this thread.
        if let Err(e) = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } {
            warn!("WTSRegisterSessionNotification failed: {e}; session lock detection disabled");
            // SAFETY: `hwnd` is the window we just created on this thread.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return None;
        }
        info!("SessionNotificationRegistered");
        Some(Self { hwnd })
    }

    /// Unregister notifications and destroy the window. Must run on the
    /// creating thread (it does: called from `hook_thread_main`).
    fn destroy(self) {
        // SAFETY: `hwnd` is the live window created by this thread.
        unsafe {
            let _ = WTSUnRegisterSessionNotification(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Session window procedure. Runs on the monitor thread during
/// `DispatchMessageW`. Handles lock/unlock and defers everything else.
unsafe extern "system" fn session_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_WTSSESSION_CHANGE {
        match wparam.0 as u32 {
            WTS_SESSION_LOCK => handle_session_lock(),
            WTS_SESSION_UNLOCK => handle_session_unlock(),
            _ => {}
        }
        return LRESULT(0);
    }
    // SAFETY: default processing with the exact message we received.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// On session lock: force-reset the combo (closing any active dictation
/// with `ReleaseReason::SessionLocked`) and announce the lock.
fn handle_session_lock() {
    with_hook_state(|state| {
        let out = state.tracker.reset();
        // Emits ComboReleased{SessionLocked} only if a combo was active.
        state.publish_tracker_output(out);
        state.emit(HotkeyEvent::SessionLocked);
    });
    // Literal "SESSION_LOCKED" is grep-able evidence for FR-01.AC-6.
    info!("SESSION_LOCKED");
}

/// On session unlock: announce it. The tracker was already idle (reset
/// on lock), so no combo state carries across.
fn handle_session_unlock() {
    with_hook_state(|state| {
        state.emit(HotkeyEvent::SessionUnlocked);
    });
    // Literal "SESSION_UNLOCKED" is grep-able evidence for FR-01.AC-6.
    info!("SESSION_UNLOCKED");
}

#[cfg(test)]
mod integration_tests {
    //! Synthetic-input integration tests (plan FASE 2, Task 2).
    //!
    //! WARNING: these tests inject REAL key events system-wide via
    //! `SendInput` (they pass through the process's own LL hook, which
    //! is exactly what we verify). Run them with the terminal focused
    //! and hands off the keyboard:
    //!
    //! ```text
    //! cargo test --lib hotkey -- --ignored --nocapture --test-threads=1
    //! ```

    use super::*;
    use std::mem::size_of;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };

    const VK_LCONTROL: u16 = 0xA2;
    const VK_LWIN: u16 = 0x5B;
    /// Innocuous third key: no OS shortcut uses Ctrl+Win+F13 (unlike
    /// e.g. D, which would switch virtual desktops).
    const VK_F13: u16 = 0x7C;

    fn send_key(vk: u16, up: bool) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        // SAFETY: `input` is fully initialized and SendInput copies it.
        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        assert_eq!(sent, 1, "SendInput failed for vk {vk:#04X}");
    }

    /// Mandatory cleanup guard: releases every VK the tests inject even
    /// if the test panics mid-combo. Without it, a dying test would
    /// leave Ctrl/Win "stuck down" at OS level.
    struct KeyCleanup;

    impl Drop for KeyCleanup {
        fn drop(&mut self) {
            for vk in [VK_LCONTROL, VK_LWIN, VK_F13] {
                send_key(vk, true);
            }
        }
    }

    fn recv_event(rx: &Receiver<HotkeyEvent>, what: &str) -> HotkeyEvent {
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|e| panic!("timed out waiting for {what}: {e}"))
    }

    #[test]
    #[ignore = "injects real key events; run focused, --test-threads=1"]
    fn hotkey_synthetic_press_release() {
        let (tx, rx) = channel();
        let handle = HotkeyMonitor::start(tx).expect("monitor should start");
        let cleanup = KeyCleanup;

        send_key(VK_LCONTROL, false);
        send_key(VK_LWIN, false);
        std::thread::sleep(Duration::from_millis(300));
        send_key(VK_LWIN, true);

        assert_eq!(recv_event(&rx, "ComboPressed"), HotkeyEvent::ComboPressed);
        match recv_event(&rx, "ComboReleased") {
            HotkeyEvent::ComboReleased { hold_ms, reason } => {
                assert!(
                    (200..=800).contains(&hold_ms),
                    "hold_ms out of expected range (200..=800): {hold_ms}"
                );
                assert_eq!(reason, ReleaseReason::KeyLifted);
            }
            other => panic!("expected ComboReleased, got {other:?}"),
        }

        drop(cleanup);
        handle.stop();
    }

    #[test]
    #[ignore = "injects real key events; run focused, --test-threads=1"]
    fn hotkey_third_key_cancels() {
        let (tx, rx) = channel();
        let handle = HotkeyMonitor::start(tx).expect("monitor should start");
        let cleanup = KeyCleanup;

        send_key(VK_LCONTROL, false);
        send_key(VK_LWIN, false);
        assert_eq!(recv_event(&rx, "ComboPressed"), HotkeyEvent::ComboPressed);

        send_key(VK_F13, false);
        match recv_event(&rx, "ComboReleased(OtherKeyPressed)") {
            HotkeyEvent::ComboReleased { reason, .. } => {
                assert_eq!(reason, ReleaseReason::OtherKeyPressed);
            }
            other => panic!("expected ComboReleased, got {other:?}"),
        }

        drop(cleanup);
        handle.stop();
    }
}
