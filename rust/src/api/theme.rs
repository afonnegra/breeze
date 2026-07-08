//! FRB theme API (FASE 6): initial taskbar theme + change stream.

use std::sync::mpsc;
use std::sync::Mutex;
use std::thread::JoinHandle;

use tracing::{debug, info, warn};

use crate::api::input::InputError;
use crate::frb_generated::StreamSink;
use crate::theme::{self, ThemeWatcher, ThemeWatcherHandle};

struct WatcherState {
    handle: ThemeWatcherHandle,
    bridge: JoinHandle<()>,
}

static WATCHER: Mutex<Option<WatcherState>> = Mutex::new(None);

/// Taskbar theme right now. Falls back to dark (false) when the
/// registry value is unreadable - dark is the Windows 11 default.
pub fn get_taskbar_light_theme() -> bool {
    theme::system_uses_light_theme().unwrap_or(false)
}

/// Streams taskbar theme changes (true = light) to Dart. One watcher
/// per process; mirrors the hotkey monitor start/stop contract.
pub fn watch_taskbar_theme(sink: StreamSink<bool>) -> Result<(), InputError> {
    let mut slot = WATCHER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_some() {
        warn!("watch_taskbar_theme: watcher already running");
        return Err(InputError::MonitorAlreadyRunning);
    }
    let (tx, rx) = mpsc::channel::<bool>();
    let handle = ThemeWatcher::start(tx)
        .map_err(InputError::MonitorStartFailed)?;
    let bridge = match std::thread::Builder::new()
        .name("theme-frb-bridge".into())
        .spawn(move || {
            while let Ok(light) = rx.recv() {
                info!(light, "TaskbarThemeChanged");
                if sink.add(light).is_err() {
                    debug!("theme bridge: sink closed, event dropped");
                }
            }
            debug!("theme bridge thread exiting (channel closed)");
        }) {
        Ok(join) => join,
        Err(e) => {
            handle.stop();
            return Err(InputError::MonitorStartFailed(e.to_string()));
        }
    };
    *slot = Some(WatcherState { handle, bridge });
    info!("watch_taskbar_theme: watcher started");
    Ok(())
}

/// Stops the theme watcher (symmetry / test hygiene; the app itself
/// keeps it for the process lifetime).
pub fn stop_taskbar_theme_watcher() -> Result<(), InputError> {
    let state = WATCHER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or(InputError::MonitorNotRunning)?;
    state.handle.stop();
    if state.bridge.join().is_err() {
        warn!("stop_taskbar_theme_watcher: bridge thread panicked");
    }
    Ok(())
}
