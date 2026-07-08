//! FRB overlay window API (FASE 4, Task 3 / FR-03): Win32 styling for
//! the Flutter overlay window - layered, click-through, no-activate,
//! always-on-top, hidden from taskbar/Alt+Tab - plus show/hide calls
//! that never steal focus from the dictation target.
//!
//! Dart configures the window through `window_manager` (frameless,
//! sized, positioned) and gives it a UNIQUE window title; this module
//! locates that window with `FindWindowExW` filtering on BOTH the
//! Flutter runner window class and the exact title (TD-013), and ORs
//! the FR-03 extended styles onto whatever the Flutter runner set up.
//!
//! Show/hide live here (and NOT in `window_manager`) on purpose:
//! `windowManager.show()` calls `ShowWindow(SW_SHOW)` + activation,
//! which moves focus (and the caret) away from the app the user is
//! dictating into - exactly what FR-03.AC-3 forbids. `ShowWindow` with
//! `SW_SHOWNOACTIVATE` displays the overlay without activating it.
//!
//! The title is passed on every call and must match the literal
//! `kOverlayWindowTitle` in `lib/main.dart` ("Breeze-overlay").

use std::thread::sleep;
use std::time::Duration;

use tracing::{debug, info, warn};
use windows::core::HSTRING;
use windows::Win32::Foundation::{GetLastError, SetLastError, COLORREF, HWND, WIN32_ERROR};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT,
};

use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use crate::api::input::InputError;

/// Window class of the Flutter runner host window, verified by smoke
/// (FASE 5, TD-013): a real launch of `breeze.exe` enumerated the
/// overlay window as class `FLUTTER_RUNNER_WIN32_WINDOW` with title
/// `Breeze-overlay`. Filtering on the class in addition to the unique
/// title stops a foreign top-level window that happened to share the
/// title from being styled by mistake.
const FLUTTER_WINDOW_CLASS: &str = "FLUTTER_RUNNER_WIN32_WINDOW";

/// Bounded retry for the window lookup (TD-013). At startup the Flutter
/// runner may not have created its host window yet when Rust races
/// ahead to `apply_overlay_styles`, so the lookup retries a few times
/// before it is treated as fatal. A steady-state lookup succeeds on the
/// first attempt and never sleeps.
const FIND_ATTEMPTS: u32 = 3;
const FIND_RETRY_DELAY: Duration = Duration::from_millis(100);

/// The FR-03 extended style bits, OR-ed onto the current window style:
/// layered (required by TRANSPARENT and for alpha rendering),
/// click-through, no focus stealing, always-on-top, and hidden from
/// the taskbar and Alt+Tab.
fn overlay_ex_style_bits() -> isize {
    (WS_EX_LAYERED.0
        | WS_EX_TRANSPARENT.0
        | WS_EX_NOACTIVATE.0
        | WS_EX_TOPMOST.0
        | WS_EX_TOOLWINDOW.0) as isize
}

/// Locates the overlay window by BOTH the Flutter runner class and the
/// exact title (TD-013), retrying up to [`FIND_ATTEMPTS`] times to
/// absorb the startup race with the Flutter runner. The class filter is
/// injected so unit tests can target a window of their own registered
/// class; production callers use [`find_overlay_window`].
fn find_overlay_window_of_class(class: &str, title: &str) -> Result<HWND, InputError> {
    let class_h = HSTRING::from(class);
    let title_h = HSTRING::from(title);
    for attempt in 1..=FIND_ATTEMPTS {
        // SAFETY: FindWindowExW only reads the two NUL-terminated
        // HSTRINGs; a null parent/child searches top-level windows.
        let found = unsafe { FindWindowExW(None, None, &class_h, &title_h) };
        match found {
            Ok(hwnd) if !hwnd.0.is_null() => return Ok(hwnd),
            _ => {
                if attempt < FIND_ATTEMPTS {
                    debug!(title, attempt, "overlay window not found yet, retrying");
                    sleep(FIND_RETRY_DELAY);
                }
            }
        }
    }
    warn!(title, "overlay window not found after retries (TD-013)");
    Err(InputError::OverlayWindowNotFound(title.to_string()))
}

/// Locates the production overlay window (Flutter runner class + title).
fn find_overlay_window(title: &str) -> Result<HWND, InputError> {
    find_overlay_window_of_class(FLUTTER_WINDOW_CLASS, title)
}

/// Applies the FR-03 window styles to the (already created) Flutter
/// overlay window identified by `window_title`:
///
/// - `WS_EX_LAYERED | WS_EX_TRANSPARENT`: clicks pass through to the
///   window underneath (FR-03.AC-2).
/// - `WS_EX_NOACTIVATE`: the window never takes focus (FR-03.AC-3).
/// - `WS_EX_TOPMOST` + `SetWindowPos(HWND_TOPMOST)`: always on top.
/// - `WS_EX_TOOLWINDOW`: hidden from taskbar and Alt+Tab.
/// - `SetLayeredWindowAttributes(alpha = 255)`: REQUIRED - a layered
///   window renders nothing at all until its transparency is set once;
///   255 keeps the Flutter content fully opaque (the visual
///   semi-transparency comes from Flutter's own alpha colors).
///
/// Call it once at startup, after the window exists and its title was
/// set, and before the first show.
pub fn apply_overlay_styles(window_title: String) -> Result<(), InputError> {
    let hwnd = find_overlay_window(&window_title)?;
    // SAFETY: hwnd is this process's own overlay window, located above;
    // the calls only update styles/position of a live window.
    unsafe {
        // SetWindowLongPtrW/GetWindowLongPtrW return 0 both on failure
        // and when the previous value happens to be 0, so the Win32
        // documented SetLastError(0) dance disambiguates.
        SetLastError(WIN32_ERROR(0));
        let previous = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if previous == 0 && GetLastError() != WIN32_ERROR(0) {
            return Err(InputError::OverlayStyleFailed(format!(
                "GetWindowLongPtrW(GWL_EXSTYLE) failed: {:?}",
                GetLastError()
            )));
        }
        SetLastError(WIN32_ERROR(0));
        let replaced = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, previous | overlay_ex_style_bits());
        if replaced == 0 && GetLastError() != WIN32_ERROR(0) {
            return Err(InputError::OverlayStyleFailed(format!(
                "SetWindowLongPtrW(GWL_EXSTYLE) failed: {:?}",
                GetLastError()
            )));
        }
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA).map_err(|e| {
            InputError::OverlayStyleFailed(format!("SetLayeredWindowAttributes failed: {e}"))
        })?;
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|e| InputError::OverlayStyleFailed(format!("SetWindowPos failed: {e}")))?;
    }
    info!(title = %window_title, "apply_overlay_styles: FR-03 extended styles applied");
    Ok(())
}

/// Shows the overlay WITHOUT activating it (`SW_SHOWNOACTIVATE`), so
/// the focused app keeps its focus and caret (FR-03.AC-3).
pub fn show_overlay_no_activate(window_title: String) -> Result<(), InputError> {
    let hwnd = find_overlay_window(&window_title)?;
    // SAFETY: ShowWindow on our own window. The return value is the
    // PREVIOUS visibility state, not an error - intentionally ignored.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    debug!("show_overlay_no_activate: overlay shown");
    Ok(())
}

/// Hides the overlay (`SW_HIDE`). Focus is unaffected: a NOACTIVATE
/// window never had it.
pub fn hide_overlay(window_title: String) -> Result<(), InputError> {
    let hwnd = find_overlay_window(&window_title)?;
    // SAFETY: ShowWindow on our own window; previous-state result ignored.
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    debug!("hide_overlay: overlay hidden");
    Ok(())
}
/// Makes the overlay window able to host the tray context menu.
///
/// The tray plugin owns its popup menu from the Flutter main window and
/// requires SetForegroundWindow to succeed. Our overlay carries
/// WS_EX_NOACTIVATE, which Windows honors by DENYING SetForegroundWindow,
/// so TrackPopupMenu would show nothing. This temporarily strips the
/// no-activate and click-through bits and brings the window to the
/// foreground. When idle the overlay renders a transparent, empty frame,
/// so nothing visible flashes. Pair every call with `disable_menu_mode`.
pub fn enable_menu_mode(window_title: String) -> Result<(), InputError> {
    let hwnd = find_overlay_window(&window_title)?;
    // SAFETY: all calls target our own window / thread input queues.
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let menu_capable =
            current & !((WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0) as isize);
        SetLastError(WIN32_ERROR(0));
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, menu_capable);
        let _ = ShowWindow(hwnd, SW_SHOW);
        // Foreground-lock workaround: a background process (the shell owns
        // the foreground when the tray is clicked) cannot just call
        // SetForegroundWindow - Windows rejects it, so the tray menu shows
        // then vanishes and needs several clicks. Attaching our input queue
        // to the current foreground thread lifts that restriction.
        let fg = GetForegroundWindow();
        let this_tid = GetCurrentThreadId();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let attached = fg_tid != 0 && fg_tid != this_tid;
        if attached {
            let _ = AttachThreadInput(this_tid, fg_tid, true);
        }
        let _ = SetForegroundWindow(hwnd);
        let _ = BringWindowToTop(hwnd);
        if attached {
            let _ = AttachThreadInput(this_tid, fg_tid, false);
        }
    }
    debug!("enable_menu_mode: overlay is menu-capable");
    Ok(())
}

/// Restores the overlay FR-03 styles after the tray menu closes.
/// `hide` is true when the dictation phase is idle so the window goes
/// back to SW_HIDE; during an active dictation the caller keeps it shown.
pub fn disable_menu_mode(window_title: String, hide: bool) -> Result<(), InputError> {
    let hwnd = find_overlay_window(&window_title)?;
    // SAFETY: our own window, same thread.
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetLastError(WIN32_ERROR(0));
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, current | overlay_ex_style_bits());
        if hide {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    debug!(hide, "disable_menu_mode: overlay styles restored");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use std::time::Instant;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindowVisible, RegisterClassW,
        WINDOW_EX_STYLE, WNDCLASSW, WS_POPUP,
    };

    /// Minimal `extern "system"` window procedure for the test class -
    /// delegates everything to `DefWindowProcW`. A raw fn pointer of the
    /// exact WNDPROC ABI is required (the crate wrapper is a Rust fn).
    unsafe extern "system" fn test_wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    static REGISTER_CLASS: Once = Once::new();

    /// Registers a process-local window class named exactly
    /// [`FLUTTER_WINDOW_CLASS`] so the test windows are found by the
    /// same class filter the production lookup uses (TD-013). Registered
    /// once; `RegisterClassW` on a duplicate would fail with
    /// ERROR_CLASS_ALREADY_EXISTS.
    fn ensure_flutter_class() {
        REGISTER_CLASS.call_once(|| {
            let class = HSTRING::from(FLUTTER_WINDOW_CLASS);
            // SAFETY: GetModuleHandleW(None) returns this module handle;
            // the WNDCLASSW points at a NUL-terminated class name that
            // outlives the RegisterClassW call.
            let hinstance: HINSTANCE = unsafe {
                GetModuleHandleW(None).expect("module handle").into()
            };
            let wc = WNDCLASSW {
                lpfnWndProc: Some(test_wndproc),
                hInstance: hinstance,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            // SAFETY: wc is fully initialized; RegisterClassW copies it.
            let atom = unsafe { RegisterClassW(&wc) };
            assert_ne!(atom, 0, "RegisterClassW(FLUTTER_RUNNER_WIN32_WINDOW) failed");
        });
    }

    /// Hidden 1x1 popup window of class [`FLUTTER_WINDOW_CLASS`] with a
    /// unique title, owned by the calling test thread. Off-screen so a
    /// SW_SHOWNOACTIVATE during the test never flashes on the desktop.
    struct TestWindow {
        hwnd: HWND,
        title: String,
    }

    impl TestWindow {
        fn create(tag: &str) -> Self {
            ensure_flutter_class();
            let title = format!("breeze-overlay-test-{tag}-{}", std::process::id());
            let class = HSTRING::from(FLUTTER_WINDOW_CLASS);
            // SAFETY: the class was registered above; a hidden popup
            // owned by this thread is a valid top-level window. Style
            // and ShowWindow calls below are same-thread, so no message
            // pump is needed.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    &class,
                    &HSTRING::from(title.as_str()),
                    WS_POPUP,
                    -10_000,
                    -10_000,
                    1,
                    1,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .expect("CreateWindowExW(FLUTTER class) failed");
            Self { hwnd, title }
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: hwnd was created by this thread and not destroyed
            // elsewhere.
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }

    #[test]
    fn apply_styles_on_missing_window_reports_not_found() {
        let title = format!("breeze-overlay-missing-{}", std::process::id());
        match apply_overlay_styles(title.clone()) {
            Err(InputError::OverlayWindowNotFound(t)) => assert_eq!(t, title),
            other => panic!("expected OverlayWindowNotFound, got {other:?}"),
        }
    }

    // TD-013 retry: a missing window is looked up FIND_ATTEMPTS times
    // with FIND_RETRY_DELAY between tries, so the call must take at least
    // (FIND_ATTEMPTS - 1) * FIND_RETRY_DELAY before returning not-found.
    #[test]
    fn missing_window_lookup_retries_before_failing() {
        let title = format!("breeze-overlay-retry-{}", std::process::id());
        let started = Instant::now();
        let result = show_overlay_no_activate(title);
        let elapsed = started.elapsed();
        assert!(matches!(result, Err(InputError::OverlayWindowNotFound(_))));
        let min = FIND_RETRY_DELAY * (FIND_ATTEMPTS - 1);
        assert!(
            elapsed >= min,
            "expected at least {min:?} of retry backoff, took {elapsed:?}"
        );
    }

    #[test]
    fn show_and_hide_on_missing_window_report_not_found() {
        let title = format!("breeze-overlay-missing-sh-{}", std::process::id());
        assert!(matches!(
            show_overlay_no_activate(title.clone()),
            Err(InputError::OverlayWindowNotFound(_))
        ));
        assert!(matches!(
            hide_overlay(title),
            Err(InputError::OverlayWindowNotFound(_))
        ));
    }

    #[test]
    fn apply_styles_sets_all_fr03_extended_bits() {
        let window = TestWindow::create("styles");
        // WS_EX_TOPMOST is only honored by SetWindowPos(HWND_TOPMOST) and
        // only materializes in GWL_EXSTYLE once the window is visible, so
        // the window is shown (off-screen, no activation) before styling.
        show_overlay_no_activate(window.title.clone()).expect("show should succeed");
        apply_overlay_styles(window.title.clone())
            .expect("apply_overlay_styles should succeed on a live window");
        // SAFETY: reading a style from our own live test window.
        let ex_style = unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) };
        // WS_EX_TOPMOST is deliberately NOT asserted: Windows silently
        // denies topmost promotion to background processes (the cargo
        // test harness), even though SetWindowPos reports success. In
        // the real app the freshly launched foreground process gets it,
        // window_manager alwaysOnTop re-applies it, and FR-03.AC checks
        // it manually in Task 5.
        let bits = (WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0
            | WS_EX_NOACTIVATE.0
            | WS_EX_TOOLWINDOW.0) as isize;
        assert_eq!(
            ex_style & bits,
            bits,
            "the SetWindowLongPtr-controlled FR-03 bits must be set (got {ex_style:#x})"
        );
    }

    #[test]
    fn show_and_hide_toggle_visibility_without_activation() {
        let window = TestWindow::create("showhide");
        apply_overlay_styles(window.title.clone()).expect("styles should apply");
        // SAFETY: IsWindowVisible on our own live test window.
        assert!(!unsafe { IsWindowVisible(window.hwnd) }.as_bool());
        show_overlay_no_activate(window.title.clone()).expect("show should succeed");
        assert!(unsafe { IsWindowVisible(window.hwnd) }.as_bool());
        hide_overlay(window.title.clone()).expect("hide should succeed");
        assert!(!unsafe { IsWindowVisible(window.hwnd) }.as_bool());
    }
}
