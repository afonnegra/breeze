//! FRB single-instance API (TD-011): guarantees ONE running instance
//! per interactive user session.
//!
//! # Why this exists
//!
//! Each running process installs its own low-level keyboard hook
//! (`WH_KEYBOARD_LL`) to detect the Ctrl+Win combo. If the user
//! accidentally launches the app twice, BOTH hooks fire on every
//! dictation, so the capture runs twice and the transcribed text is
//! pasted twice. The fix is a classic named-mutex single-instance
//! guard: the first process creates the mutex and keeps it alive for
//! its whole lifetime; any later process sees `ERROR_ALREADY_EXISTS`
//! and must exit before initializing engine/audio/hook.
//!
//! # Scope: `Local\` not `Global\`
//!
//! The mutex name is prefixed with `Local\`, so the namespace is the
//! caller's session (terminal-services session). That is EXACTLY the
//! guarantee we want ("one instance per user session") and it needs no
//! special privileges. `Global\` would block across sessions too, but
//! creating a Global object can require elevated rights and would stop
//! a second, legitimate user (fast user switching) from running their
//! own copy - not desired.

use std::sync::OnceLock;

use tracing::{info, warn};
use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

/// Holds the single-instance mutex HANDLE for the whole process
/// lifetime. The kernel releases a named mutex only when its LAST
/// handle is closed; keeping the handle here (never dropped, never
/// closed) is what keeps the name reserved until the process exits, at
/// which point Windows reclaims the handle automatically. A raw HANDLE
/// is not `Sync`, so it is wrapped as a `usize` bit pattern; it is only
/// ever stored, never dereferenced.
static INSTANCE_MUTEX: OnceLock<usize> = OnceLock::new();

/// Attempts to become the single instance for this user session.
///
/// Returns `true` if THIS process is the first (and only) instance and
/// may continue starting up. Returns `false` if another instance
/// already owns the guard, in which case the caller MUST exit without
/// installing the keyboard hook / audio / engine (see TD-011).
///
/// Idempotent within a process: the handle is created at most once and
/// cached in [`INSTANCE_MUTEX`]. A second call in the same process
/// returns `true` (this process already IS the instance) without
/// creating another handle.
pub fn acquire_single_instance() -> bool {
    // Already acquired earlier in this process: we are the instance.
    if INSTANCE_MUTEX.get().is_some() {
        return true;
    }

    // SAFETY: `CreateMutexW` reads the static NUL-terminated wide string
    // literal produced by `w!` and returns an owned kernel handle. We
    // pass `None` for the security attributes (default) and `TRUE` to
    // request initial ownership. The returned handle is stored below and
    // intentionally never closed, so it stays valid for the whole
    // process; `GetLastError` is read immediately after, before any
    // other Win32 call can overwrite the thread's last-error code.
    let (handle, already_exists) = unsafe {
        let handle = CreateMutexW(None, true, w!("Local\\Breeze-single-instance-mutex"));
        let last_error = GetLastError();
        (handle, last_error == ERROR_ALREADY_EXISTS)
    };

    let handle: HANDLE = match handle {
        Ok(h) => h,
        Err(e) => {
            // Creating the mutex failed outright. Fail OPEN: allowing the
            // app to start is less harmful than blocking the only
            // instance because a guard could not be created.
            warn!(error = %e, "acquire_single_instance: CreateMutexW failed; allowing startup");
            return true;
        }
    };

    if already_exists {
        // Another instance created the mutex first. Our own handle is
        // dropped here (its Drop does nothing; the OS closes it on exit),
        // which is fine: the FIRST instance's handle keeps the name
        // alive. Report "not the instance" so the caller exits.
        info!("acquire_single_instance: another instance is already running");
        return false;
    }

    // We are the first instance. Keep the handle alive for the whole
    // process by parking its bit pattern in the OnceLock.
    let _ = INSTANCE_MUTEX.set(handle.0 as usize);
    info!("acquire_single_instance: this process is the single instance");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Within a single process the first call must win and every
    /// subsequent call must also return `true` (idempotent: this process
    /// already owns the guard). Cross-process rejection (the second
    /// process seeing `false`) is verified by the release-build double
    /// launch smoke test, which cannot be expressed as a single-process
    /// unit test because both "instances" would share this process's
    /// handle table and the OnceLock.
    #[test]
    fn first_call_acquires_and_is_idempotent() {
        assert!(acquire_single_instance(), "first acquisition must succeed");
        assert!(
            acquire_single_instance(),
            "same process re-acquiring must stay the instance"
        );
    }
}
