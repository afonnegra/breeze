//! Windows taskbar theme detection (FASE 6, TD-014).
//!
//! The tray icons must contrast with the taskbar, whose colors follow
//! the registry value `SystemUsesLightTheme` (NOT `AppsUseLightTheme`,
//! which drives app windows). A hidden top-level window (same pattern
//! as `hotkey::SessionWindow`) listens for
//! `WM_SETTINGCHANGE("ImmersiveColorSet")` and re-reads the value,
//! publishing changes over an mpsc channel.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::thread::JoinHandle;

use tracing::{error, info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostThreadMessageW, RegisterClassW, TranslateMessage, CW_USEDEFAULT, MSG, WINDOW_EX_STYLE,
    WM_QUIT, WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

/// Reads `HKCU\...\Themes\Personalize\SystemUsesLightTheme`.
/// `None` when the value is missing or unreadable (very old builds);
/// callers should fall back to dark (the Windows 11 default taskbar).
pub fn system_uses_light_theme() -> Option<bool> {
    let mut data: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: out-pointers reference locals that outlive the call.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    if status.is_ok() {
        Some(data != 0)
    } else {
        None
    }
}

/// Window class + title for the hidden theme-notification window.
const THEME_CLASS_NAME: PCWSTR = w!("BreezeThemeWatcher");
const THEME_WINDOW_TITLE: PCWSTR = w!("Breeze theme sink");

/// Per-watcher-thread state consumed by the theme window `WndProc`.
/// Holds the change channel and the last value published to dedupe the
/// storm of `WM_SETTINGCHANGE` Windows fires for a single theme toggle.
struct ThemeThreadState {
    tx: Sender<bool>,
    last_published: bool,
}

thread_local! {
    /// WndProc context. Only ever populated on the watcher thread; same
    /// mechanism as the hotkey hook state (`hotkey::HOOK_STATE`).
    static THEME_STATE: RefCell<Option<ThemeThreadState>> = const { RefCell::new(None) };
}

/// Runs `f` with mutable access to the watcher-thread state, if it is
/// initialized. A no-op off-thread or before init. Never panics: a
/// WndProc must not unwind into Win32.
fn with_theme_state<F: FnOnce(&mut ThemeThreadState)>(f: F) {
    THEME_STATE.with(|cell| {
        if let Ok(mut borrow) = cell.try_borrow_mut() {
            if let Some(state) = borrow.as_mut() {
                f(state);
            }
        }
    });
}

/// Theme watcher: a hidden top-level window on a dedicated thread whose
/// `WndProc` re-reads `system_uses_light_theme()` on
/// `WM_SETTINGCHANGE("ImmersiveColorSet")` and publishes real changes.
pub struct ThemeWatcher;

impl ThemeWatcher {
    /// Start the watcher thread; taskbar theme changes (true = light)
    /// are sent to `tx`. Mirrors `hotkey::HotkeyMonitor::start`.
    pub fn start(tx: Sender<bool>) -> Result<ThemeWatcherHandle, String> {
        // The thread reports its init outcome (thread id, or error)
        // through this one-shot channel before entering the pump.
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let join = std::thread::Builder::new()
            .name("theme-watcher".into())
            .spawn(move || watcher_thread_main(tx, init_tx))
            .map_err(|e| e.to_string())?;

        match init_rx.recv() {
            Ok(Ok(thread_id)) => Ok(ThemeWatcherHandle {
                thread_id,
                join: Some(join),
            }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                let _ = join.join();
                Err("theme watcher thread exited before reporting initialization".into())
            }
        }
    }
}

/// Handle owning the watcher thread. Stops it on `stop()` or on drop.
pub struct ThemeWatcherHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl ThemeWatcherHandle {
    /// Stop the watcher: post `WM_QUIT` to the watcher thread's message
    /// pump and join it. The window is destroyed by the thread itself.
    /// Same contract as `hotkey::HotkeyMonitorHandle::stop`.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        // SAFETY: plain Win32 call; the target thread owns a message
        // queue (it created the window before start() returned), so
        // posting WM_QUIT is always valid while it is alive.
        if let Err(e) = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        {
            warn!("PostThreadMessage(WM_QUIT) to theme thread failed: {e}");
        }
        if join.join().is_err() {
            error!("theme watcher thread panicked");
        }
    }
}

impl Drop for ThemeWatcherHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Body of the dedicated watcher thread: create the window, seed the
/// dedup cache, pump messages, tear the window down.
fn watcher_thread_main(tx: Sender<bool>, init_tx: Sender<Result<u32, String>>) {
    // Seed the last-published value with the current theme so the first
    // real toggle publishes and identical WM_SETTINGCHANGE storms don't.
    let initial = system_uses_light_theme().unwrap_or(false);

    // Populate the WndProc context BEFORE creating the window so the
    // proc can never observe a half-initialized state.
    THEME_STATE.with(|cell| {
        *cell.borrow_mut() = Some(ThemeThreadState {
            tx,
            last_published: initial,
        });
    });

    // SAFETY: plain Win32 call; a null module handle is never returned
    // for the current process.
    let module = unsafe { GetModuleHandleW(None) };
    let hinstance: HINSTANCE = match module {
        Ok(m) => m.into(),
        Err(e) => {
            let _ = init_tx.send(Err(format!("GetModuleHandleW failed: {e}")));
            return;
        }
    };

    let window = match ThemeWindow::create(hinstance) {
        Some(w) => w,
        None => {
            let _ = init_tx.send(Err("theme watcher window creation failed".into()));
            return;
        }
    };

    // SAFETY: plain Win32 call, no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    if init_tx.send(Ok(thread_id)).is_err() {
        // start() vanished; nobody owns us. Tear down and bail out.
        window.destroy();
        return;
    }

    run_message_pump();

    window.destroy();
}

/// Blocking message pump; returns when `WM_QUIT` is received. The theme
/// window's `WndProc` is invoked by the OS while this thread waits
/// inside `GetMessageW`. Mirrors `hotkey::run_message_pump`.
fn run_message_pump() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid, writable MSG for the whole call.
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        match ret.0 {
            0 => break, // WM_QUIT
            -1 => {
                error!(
                    "GetMessageW failed in theme pump: {}",
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

/// One-time registration of the theme window class for this process.
/// A `OnceLock` is correct: the class lives for the process lifetime
/// and must be registered exactly once even across stop/start cycles.
/// Mirrors `hotkey::ensure_session_class`.
static THEME_CLASS_REGISTERED: OnceLock<bool> = OnceLock::new();

fn ensure_theme_class(hinstance: HINSTANCE) -> bool {
    *THEME_CLASS_REGISTERED.get_or_init(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(theme_wnd_proc),
            hInstance: hinstance,
            lpszClassName: THEME_CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: `wc` is fully initialized and lives for the call;
        // `lpszClassName` is a 'static wide string.
        let atom = unsafe { RegisterClassW(&wc) };
        if atom == 0 {
            warn!(
                "RegisterClassW for theme window failed: {}; theme watching disabled",
                windows::core::Error::from_thread()
            );
            false
        } else {
            true
        }
    })
}

/// Hidden top-level window that receives `WM_SETTINGCHANGE`. A NORMAL
/// top-level window (never shown), not `HWND_MESSAGE`: broadcast
/// messages like `WM_SETTINGCHANGE` are NOT delivered to message-only
/// windows. Same rationale as `hotkey::SessionWindow`.
struct ThemeWindow {
    hwnd: HWND,
}

impl ThemeWindow {
    /// Create the hidden window. Returns `None` (logged) on any failure.
    fn create(hinstance: HINSTANCE) -> Option<Self> {
        if !ensure_theme_class(hinstance) {
            return None;
        }
        // Never shown (no ShowWindow), so it stays invisible. NORMAL
        // top-level window (not HWND_MESSAGE) so WM_SETTINGCHANGE
        // broadcasts are delivered.
        // SAFETY: class is registered; all handles are valid/optional.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                THEME_CLASS_NAME,
                THEME_WINDOW_TITLE,
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
        match hwnd {
            Ok(hwnd) => {
                info!("ThemeWatcherWindowCreated");
                Some(Self { hwnd })
            }
            Err(e) => {
                warn!("CreateWindowExW for theme window failed: {e}; theme watching disabled");
                None
            }
        }
    }

    /// Destroy the window. Must run on the creating thread (it does:
    /// called from `watcher_thread_main`).
    fn destroy(self) {
        // SAFETY: `hwnd` is the live window created by this thread.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Theme window procedure. Runs on the watcher thread during
/// `DispatchMessageW`. On `WM_SETTINGCHANGE("ImmersiveColorSet")`
/// re-reads the theme and publishes only real changes (Windows fires
/// several WM_SETTINGCHANGE per toggle, so dedup is required).
unsafe extern "system" fn theme_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_SETTINGCHANGE && lparam_is_immersive_color_set(lparam) {
        if let Some(light) = system_uses_light_theme() {
            with_theme_state(|state| {
                if light != state.last_published {
                    state.last_published = light;
                    // A closed receiver just means nobody is listening;
                    // the watcher keeps running until stopped.
                    let _ = state.tx.send(light);
                }
            });
        }
        return LRESULT(0);
    }
    // SAFETY: default processing with the exact message we received.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// True when a `WM_SETTINGCHANGE` `lparam` wide string equals
/// `"ImmersiveColorSet"` (the section Windows names on a theme toggle).
fn lparam_is_immersive_color_set(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    // SAFETY: for WM_SETTINGCHANGE with a non-null lParam, it points to
    // a NUL-terminated wide string for the duration of the call (Win32
    // contract). We read until NUL, bounded, without retaining it.
    let ptr = lparam.0 as *const u16;
    let target = "ImmersiveColorSet";
    let target_units: Vec<u16> = target.encode_utf16().collect();
    for (i, expected) in target_units.iter().enumerate() {
        let ch = unsafe { *ptr.add(i) };
        if ch != *expected {
            return false;
        }
    }
    // The message string must end exactly here (NUL right after).
    let terminator = unsafe { *ptr.add(target_units.len()) };
    terminator == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_real_personalize_value() {
        // On every supported Windows the value exists; the test pins
        // that the plumbing returns Some (either theme is fine).
        assert!(system_uses_light_theme().is_some());
    }
}
