//! FRB input API (FR-01 + FR-02 + FR-05): hotkey event stream,
//! microphone capture control, the capture-and-transcribe pipeline
//! preview, and text injection into the focused window.
//!
//! Dart subscribes to hotkey combo transitions via a `StreamSink` and
//! drives the pre-warmed [`AudioCapture`] through start/stop calls. The
//! PCM buffer never crosses the FFI boundary in the normal flow:
//! [`audio_stop_capture_and_transcribe`] chains capture -> Whisper
//! engine entirely on the Rust side and only the transcribed text
//! reaches Dart. That function is the preview of the full dictation
//! pipeline (the PHASE 4 orchestrator will own this flow natively).
//!
//! # Design notes
//!
//! - **Monitor state lives in a `Mutex<Option<...>>`, not a `OnceLock`:**
//!   FR-11 pause/resume needs stop/start cycles, and `OnceLock` is
//!   write-once.
//! - **A bridge thread pumps `mpsc::Receiver` -> `StreamSink`.** The
//!   hotkey hook thread publishes to a plain std channel (it must never
//!   block on FFI); the bridge forwards each event to Dart with
//!   `sink.add`. When the monitor stops, the hook thread drops its
//!   `Sender`, `recv()` errors out and the bridge exits on its own.
//! - **`cpal::Stream` is `Send` on Windows** (WASAPI `unsafe impl Send`,
//!   cpal 0.18.1 `src/host/wasapi/stream.rs:230`), so `AudioCapture`
//!   can live in a process-wide `Mutex` and be touched from FRB's
//!   worker threads.

use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait};
use tracing::{debug, info, warn};

use crate::api::transcription::{transcribe_pcm, TranscriptionError, TranscriptionLanguage};
use crate::audio::{device_watch_action, AudioCapture, CaptureError, WatchAction};
use crate::frb_generated::StreamSink;
use crate::hotkey::{HotkeyError, HotkeyEvent, HotkeyMonitor, HotkeyMonitorHandle, ReleaseReason};
use crate::inject::{self, InjectError, InjectionOutcome};

/// What ended a combo, as exposed to Dart (mirror of
/// [`crate::hotkey::ReleaseReason`], kept separate so the FRB scanner
/// only walks `api::` types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseReasonDto {
    /// Ctrl or Win was lifted normally.
    KeyLifted,
    /// A third key was pressed while the combo was held.
    OtherKeyPressed,
    /// The interactive session was locked (e.g. Win+L).
    SessionLocked,
}

/// Hotkey monitor event, as exposed to Dart. FRB 2.12 translates this
/// enum-with-payload into a Dart sealed class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEventDto {
    /// The Ctrl+Win combo just became active.
    ComboPressed,
    /// The Ctrl+Win combo just ended.
    ComboReleased {
        /// How long the combo was held, in milliseconds.
        hold_ms: u64,
        /// What ended the combo.
        reason: ReleaseReasonDto,
    },
    /// The interactive session was locked.
    SessionLocked,
    /// The interactive session was unlocked.
    SessionUnlocked,
}

impl From<ReleaseReason> for ReleaseReasonDto {
    fn from(reason: ReleaseReason) -> Self {
        match reason {
            ReleaseReason::KeyLifted => Self::KeyLifted,
            ReleaseReason::OtherKeyPressed => Self::OtherKeyPressed,
            ReleaseReason::SessionLocked => Self::SessionLocked,
        }
    }
}

impl From<HotkeyEvent> for HotkeyEventDto {
    fn from(event: HotkeyEvent) -> Self {
        match event {
            HotkeyEvent::ComboPressed => Self::ComboPressed,
            HotkeyEvent::ComboReleased { hold_ms, reason } => Self::ComboReleased {
                hold_ms,
                reason: reason.into(),
            },
            HotkeyEvent::SessionLocked => Self::SessionLocked,
            HotkeyEvent::SessionUnlocked => Self::SessionUnlocked,
        }
    }
}

/// Injection outcome, as exposed to Dart (mirror of
/// [`crate::inject::InjectionOutcome`], kept separate so the FRB
/// scanner only walks `api::` types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionOutcomeDto {
    /// Text pasted and the previous clipboard contents were restored.
    Pasted,
    /// Text pasted but restoring the previous clipboard failed. Not
    /// fatal - the transcription reached the target app.
    PastedRestoreFailed,
}

impl From<InjectionOutcome> for InjectionOutcomeDto {
    fn from(outcome: InjectionOutcome) -> Self {
        match outcome {
            InjectionOutcome::Pasted => Self::Pasted,
            InjectionOutcome::PastedRestoreFailed => Self::PastedRestoreFailed,
        }
    }
}

/// Typed error for the input API, exposed to Dart.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("a hotkey monitor is already running in this process")]
    MonitorAlreadyRunning,
    #[error("hotkey monitor failed to start: {0}")]
    MonitorStartFailed(String),
    #[error("no hotkey monitor is running")]
    MonitorNotRunning,
    #[error("no default audio input device available")]
    AudioNoDevice,
    #[error("audio capture failed to initialize: {0}")]
    AudioInitFailed(String),
    #[error("audio capture not pre-warmed: call audio_prewarm first")]
    AudioNotPrewarmed,
    #[error("text injection failed: {0}")]
    InjectionFailed(String),
    #[error("the orchestrator is already running")]
    OrchestratorAlreadyRunning,
    #[error("no orchestrator is running")]
    OrchestratorNotRunning,
    #[error("orchestrator failed to start: {0}")]
    OrchestratorStartFailed(String),
    #[error("overlay window not found by title: {0}")]
    OverlayWindowNotFound(String),
    #[error("overlay style update failed: {0}")]
    OverlayStyleFailed(String),
}

impl From<HotkeyError> for InputError {
    fn from(err: HotkeyError) -> Self {
        match err {
            HotkeyError::AlreadyRunning => Self::MonitorAlreadyRunning,
            HotkeyError::HookInstallFailed(_) | HotkeyError::ThreadStartFailed(_) => {
                Self::MonitorStartFailed(err.to_string())
            }
        }
    }
}

impl From<CaptureError> for InputError {
    fn from(err: CaptureError) -> Self {
        match err {
            CaptureError::NoDevice => Self::AudioNoDevice,
            CaptureError::StreamBuild(_) | CaptureError::StreamPlay(_) => {
                Self::AudioInitFailed(err.to_string())
            }
        }
    }
}

impl From<InjectError> for InputError {
    fn from(err: InjectError) -> Self {
        Self::InjectionFailed(err.to_string())
    }
}

/// Running monitor: the Win32 hook handle plus the bridge thread that
/// forwards events to the Dart sink.
struct MonitorState {
    handle: HotkeyMonitorHandle,
    bridge: JoinHandle<()>,
}

/// Process-wide monitor slot. `Mutex<Option<...>>` (NOT `OnceLock`) so
/// FR-11 pause/resume can stop and restart the monitor.
static MONITOR: Mutex<Option<MonitorState>> = Mutex::new(None);

/// Process-wide pre-warmed capture. Same re-writable rationale.
static AUDIO: Mutex<Option<AudioCapture>> = Mutex::new(None);

fn lock_monitor() -> std::sync::MutexGuard<'static, Option<MonitorState>> {
    // A poisoned lock only means another thread panicked while holding
    // it; the Option inside is still meaningful.
    MONITOR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn lock_audio() -> std::sync::MutexGuard<'static, Option<AudioCapture>> {
    AUDIO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One device watcher per process, started lazily by `audio_prewarm`.
/// Daemon thread: it lives until the process exits (quit via tray ends
/// the process; pause keeps audio prewarmed on purpose).
static DEVICE_WATCHER: OnceLock<()> = OnceLock::new();

/// NFR-11 poll cadence: 1 s comfortably beats the <= 2 s detection SLO.
const DEVICE_WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// Reads the system default input device name, or `None` when there is
/// no default device (or its name cannot be resolved). cpal 0.18.1 in
/// this tree exposes the name through `Device::description()`, not a
/// `name()` method.
fn current_default_input_name() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .and_then(|d| d.description().ok())
        .map(|desc| desc.name().to_owned())
}

fn start_device_watcher() {
    DEVICE_WATCHER.get_or_init(|| {
        let spawned = std::thread::Builder::new()
            .name("audio-device-watcher".into())
            .spawn(|| loop {
                std::thread::sleep(DEVICE_WATCH_INTERVAL);
                device_watch_tick();
            });
        if let Err(e) = spawned {
            warn!(error = %e, "device watcher: spawn failed; NFR-11 disabled this run");
        }
    });
}

/// One poll: read the system default, decide, and rebuild the
/// pre-warmed capture when it is safe to do so (never mid-dictation).
fn device_watch_tick() {
    let current = current_default_input_name();
    let action = {
        let slot = lock_audio();
        match slot.as_ref() {
            None => WatchAction::Keep, // nothing prewarmed yet
            Some(c) => device_watch_action(
                current.as_deref(),
                c.device_name(),
                c.is_recording(),
                c.is_failed(),
            ),
        }
    };
    if action != WatchAction::Rebuild {
        return;
    }
    // Build the replacement WITHOUT holding the lock (WASAPI init can
    // take a few hundred ms and must not block a hotkey press).
    match AudioCapture::prewarm() {
        Ok(fresh) => {
            let mut slot = lock_audio();
            if slot.as_ref().is_some_and(|c| c.is_recording()) {
                debug!("device watcher: dictation started mid-rebuild, keeping old stream");
                return;
            }
            let old = slot
                .as_ref()
                .map(|c| c.device_name().to_owned())
                .unwrap_or_else(|| "<none>".into());
            info!(old = %old, new = %fresh.device_name(), "AudioDeviceChanged: capture rebuilt");
            *slot = Some(fresh);
        }
        Err(e) => warn!(error = %e, "device watcher: rebuild failed, retrying next tick"),
    }
}

/// Runs `f` against the pre-warmed process-wide capture, if any.
/// Returns `None` when [`audio_prewarm`] has not run yet (FASE 4
/// orchestrator dependency wiring).
pub(crate) fn with_audio<R>(f: impl FnOnce(&AudioCapture) -> R) -> Option<R> {
    lock_audio().as_ref().map(f)
}

/// Start the global Ctrl+Win monitor and stream its events to Dart.
///
/// Fails with [`InputError::MonitorAlreadyRunning`] if a monitor is
/// already active (one low-level hook per process).
pub fn start_hotkey_monitor(sink: StreamSink<HotkeyEventDto>) -> Result<(), InputError> {
    let mut slot = lock_monitor();
    if slot.is_some() {
        warn!("start_hotkey_monitor: monitor already running");
        return Err(InputError::MonitorAlreadyRunning);
    }

    let (tx, rx) = mpsc::channel::<HotkeyEvent>();
    let handle = HotkeyMonitor::start(tx)
        .inspect_err(|e| warn!(error = %e, "start_hotkey_monitor: monitor start failed"))?;

    let bridge = match std::thread::Builder::new()
        .name("hotkey-frb-bridge".into())
        .spawn(move || {
            // Exits when the hook thread drops its Sender (monitor stop).
            while let Ok(event) = rx.recv() {
                // Per-event log for FR-01.AC-1 evidence (grep-able literals
                // "HotkeyPressed"/"HotkeyReleased" with subscriber timestamps).
                // Logged here in the bridge thread, NOT in the hook callback,
                // to respect the 1 ms callback budget (code review S2).
                match &event {
                    HotkeyEvent::ComboPressed => debug!("HotkeyPressed"),
                    HotkeyEvent::ComboReleased { hold_ms, reason } => {
                        debug!(hold_ms, ?reason, "HotkeyReleased")
                    }
                    HotkeyEvent::SessionLocked => debug!("SessionLocked event forwarded"),
                    HotkeyEvent::SessionUnlocked => debug!("SessionUnlocked event forwarded"),
                }
                let dto = HotkeyEventDto::from(event);
                if sink.add(dto).is_err() {
                    // Dart listener is gone; keep draining so the
                    // monitor never notices (it must survive UI churn).
                    debug!("hotkey bridge: sink closed, event dropped");
                }
            }
            debug!("hotkey bridge thread exiting (channel closed)");
        }) {
        Ok(join) => join,
        Err(e) => {
            // Roll back: without a bridge nobody consumes the events.
            handle.stop();
            warn!(error = %e, "start_hotkey_monitor: bridge thread spawn failed");
            return Err(InputError::MonitorStartFailed(e.to_string()));
        }
    };

    *slot = Some(MonitorState { handle, bridge });
    info!("start_hotkey_monitor: monitor started, events streaming to Dart");
    Ok(())
}

/// Stop the hotkey monitor started by [`start_hotkey_monitor`].
pub fn stop_hotkey_monitor() -> Result<(), InputError> {
    let state = lock_monitor()
        .take()
        .ok_or(InputError::MonitorNotRunning)
        .inspect_err(|_| warn!("stop_hotkey_monitor: no monitor running"))?;
    // Joins the hook thread; its thread_local Sender is dropped there,
    // which closes the channel and lets the bridge thread finish.
    state.handle.stop();
    if state.bridge.join().is_err() {
        warn!("stop_hotkey_monitor: bridge thread panicked");
    }
    info!("stop_hotkey_monitor: monitor stopped");
    Ok(())
}

/// Open and start the microphone stream so capture starts are
/// instantaneous (NFR-02). Idempotent: a second call reuses the
/// existing stream.
pub fn audio_prewarm() -> Result<(), InputError> {
    let mut slot = lock_audio();
    if slot.is_some() {
        info!("audio_prewarm: capture already pre-warmed, reusing");
        return Ok(());
    }
    let capture = AudioCapture::prewarm()
        .inspect_err(|e| warn!(error = %e, "audio_prewarm: prewarm failed"))?;
    info!(
        sample_rate = capture.sample_rate(),
        channels = capture.channels(),
        "audio_prewarm: capture ready"
    );
    *slot = Some(capture);
    start_device_watcher();
    Ok(())
}

/// Begin accumulating microphone samples (FR-02).
pub fn audio_start_capture() -> Result<(), InputError> {
    let slot = lock_audio();
    let capture = slot.as_ref().ok_or(InputError::AudioNotPrewarmed)?;
    capture.start_buffer();
    info!("audio_start_capture: buffering started");
    Ok(())
}

/// Stop accumulating and return only the PCM length in samples (UI
/// metric). The PCM itself never crosses to Dart in the normal flow.
pub fn audio_stop_capture_len() -> Result<u64, InputError> {
    let result = {
        let slot = lock_audio();
        let capture = slot.as_ref().ok_or(InputError::AudioNotPrewarmed)?;
        capture.stop_buffer()
    };
    info!(
        pcm_len = result.pcm.len(),
        audio_ms = result.duration.as_millis() as u64,
        truncated = result.truncated,
        "audio_stop_capture_len: capture closed"
    );
    Ok(result.pcm.len() as u64)
}

/// Full-pipeline preview (first real dictation): stop the capture and
/// feed the PCM straight into the resident Whisper engine, all on the
/// Rust side. Only the transcribed text crosses to Dart.
///
/// Error mapping caveat: the mandated signature returns
/// [`TranscriptionError`], so a missing `audio_prewarm` surfaces as
/// [`TranscriptionError::TranscribeFailed`] with an explanatory
/// message (there is no audio variant in that enum by design).
pub fn audio_stop_capture_and_transcribe(
    lang: TranscriptionLanguage,
) -> Result<String, TranscriptionError> {
    // Close the buffer under the lock, then transcribe WITHOUT holding
    // it: Whisper can take seconds and must not block other audio calls.
    let result = {
        let slot = lock_audio();
        let capture = slot.as_ref().ok_or_else(|| {
            warn!("audio_stop_capture_and_transcribe: capture not pre-warmed");
            TranscriptionError::TranscribeFailed(
                "audio capture not pre-warmed: call audio_prewarm first".into(),
            )
        })?;
        capture.stop_buffer()
    };
    info!(
        pcm_len = result.pcm.len(),
        audio_ms = result.duration.as_millis() as u64,
        truncated = result.truncated,
        "audio_stop_capture_and_transcribe: capture closed, transcribing"
    );
    let text = transcribe_pcm(result.pcm, lang)?;
    info!(
        text_chars = text.chars().count(),
        "audio_stop_capture_and_transcribe: transcription done"
    );
    Ok(text)
}

/// Pastes `text` into the currently focused window via the full FR-05
/// sequence (see [`crate::inject::inject`]) - wait for the physical
/// modifiers to be released, snapshot the clipboard, paste, restore.
///
/// The text is stored as the last-transcription backup BEFORE the
/// paste is attempted, so [`get_last_transcription`] can hand it back
/// even when the injection itself fails (FR-05 fallback).
///
/// NFR-12 - the text content is never logged, only its length.
pub fn inject_text(text: String) -> Result<InjectionOutcomeDto, InputError> {
    inject::store_last_transcription(&text);
    let outcome = inject::inject(&text)
        .inspect_err(|e| warn!(error = %e, "inject_text: injection failed"))?;
    info!(?outcome, len = text.len(), "inject_text: done");
    Ok(outcome.into())
}

/// Returns the last transcription kept as the FR-05 in-memory backup,
/// if any (stored by [`inject_text`]; never logged, NFR-12).
pub fn get_last_transcription() -> Option<String> {
    inject::last_transcription()
}

/// Drops the in-memory last-transcription backup (TD-008, NFR-12
/// hygiene). Wired into the ordered shutdown path: the tray Quit item
/// calls this right before the process exits.
pub fn clear_last_transcription() {
    inject::clear_last_transcription();
    info!("clear_last_transcription: backup dropped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // --- DTO mapping (plan Task 6: unit tests on the mappings) ---

    #[test]
    fn release_reason_maps_all_variants() {
        assert_eq!(
            ReleaseReasonDto::from(ReleaseReason::KeyLifted),
            ReleaseReasonDto::KeyLifted
        );
        assert_eq!(
            ReleaseReasonDto::from(ReleaseReason::OtherKeyPressed),
            ReleaseReasonDto::OtherKeyPressed
        );
        assert_eq!(
            ReleaseReasonDto::from(ReleaseReason::SessionLocked),
            ReleaseReasonDto::SessionLocked
        );
    }

    #[test]
    fn hotkey_event_maps_all_variants() {
        assert_eq!(
            HotkeyEventDto::from(HotkeyEvent::ComboPressed),
            HotkeyEventDto::ComboPressed
        );
        assert_eq!(
            HotkeyEventDto::from(HotkeyEvent::ComboReleased {
                hold_ms: 312,
                reason: ReleaseReason::OtherKeyPressed,
            }),
            HotkeyEventDto::ComboReleased {
                hold_ms: 312,
                reason: ReleaseReasonDto::OtherKeyPressed,
            }
        );
        assert_eq!(
            HotkeyEventDto::from(HotkeyEvent::SessionLocked),
            HotkeyEventDto::SessionLocked
        );
        assert_eq!(
            HotkeyEventDto::from(HotkeyEvent::SessionUnlocked),
            HotkeyEventDto::SessionUnlocked
        );
    }

    #[test]
    fn hotkey_error_maps_to_input_error() {
        assert!(matches!(
            InputError::from(HotkeyError::AlreadyRunning),
            InputError::MonitorAlreadyRunning
        ));
        assert!(matches!(
            InputError::from(HotkeyError::HookInstallFailed(-2147024891)),
            InputError::MonitorStartFailed(_)
        ));
        assert!(matches!(
            InputError::from(HotkeyError::ThreadStartFailed("x".into())),
            InputError::MonitorStartFailed(_)
        ));
    }

    #[test]
    fn capture_error_maps_to_input_error() {
        assert!(matches!(
            InputError::from(CaptureError::NoDevice),
            InputError::AudioNoDevice
        ));
        assert!(matches!(
            InputError::from(CaptureError::StreamBuild("x".into())),
            InputError::AudioInitFailed(_)
        ));
        assert!(matches!(
            InputError::from(CaptureError::StreamPlay("x".into())),
            InputError::AudioInitFailed(_)
        ));
    }

    #[test]
    fn injection_outcome_maps_all_variants() {
        assert_eq!(
            InjectionOutcomeDto::from(InjectionOutcome::Pasted),
            InjectionOutcomeDto::Pasted
        );
        assert_eq!(
            InjectionOutcomeDto::from(InjectionOutcome::PastedRestoreFailed),
            InjectionOutcomeDto::PastedRestoreFailed
        );
    }

    #[test]
    fn inject_error_maps_to_input_error() {
        assert!(matches!(
            InputError::from(InjectError::SendInputFailed(2, 4)),
            InputError::InjectionFailed(_)
        ));
        assert!(matches!(
            InputError::from(InjectError::Clipboard(
                crate::clipboard::ClipboardError::OpenTimeout
            )),
            InputError::InjectionFailed(_)
        ));
    }

    // --- Guard-rail behavior that needs no devices ---
    //
    // Safe against the process-wide statics: no unit test in this crate
    // pre-warms audio or starts a monitor (those are #[ignore]d
    // integration tests elsewhere).

    #[test]
    fn stop_monitor_without_start_is_not_running() {
        assert!(matches!(
            stop_hotkey_monitor(),
            Err(InputError::MonitorNotRunning)
        ));
    }

    #[test]
    fn audio_calls_without_prewarm_report_not_prewarmed() {
        assert!(matches!(
            audio_start_capture(),
            Err(InputError::AudioNotPrewarmed)
        ));
        assert!(matches!(
            audio_stop_capture_len(),
            Err(InputError::AudioNotPrewarmed)
        ));
        match audio_stop_capture_and_transcribe(TranscriptionLanguage::Es) {
            Err(TranscriptionError::TranscribeFailed(msg)) => {
                assert!(
                    msg.contains("audio_prewarm"),
                    "message should point at audio_prewarm: {msg}"
                );
            }
            other => panic!("expected TranscribeFailed, got {other:?}"),
        }
    }

    /// The mpsc bridge contract stop_hotkey_monitor relies on: dropping
    /// the Sender ends `recv()` with an error (bridge thread exits).
    #[test]
    fn bridge_channel_closes_when_sender_drops() {
        let (tx, rx) = mpsc::channel::<HotkeyEvent>();
        tx.send(HotkeyEvent::ComboPressed).expect("send works");
        drop(tx);
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)),
            Ok(HotkeyEvent::ComboPressed)
        );
        assert!(rx.recv().is_err(), "closed channel must end recv");
    }
}

#[cfg(test)]
mod manual_demo {
    //! Manual dictation harness (plan FASE 2, Task 7 Step 2): the
    //! user's first real end-to-end dictations, driven through the
    //! same api functions Dart will call (`audio_prewarm`,
    //! `audio_start_capture`, `audio_stop_capture_and_transcribe`).
    //!
    //! The hotkey side deliberately bypasses `start_hotkey_monitor`:
    //! that api function needs a Dart `StreamSink`, which cannot be
    //! constructed in a pure-Rust test process, so the demo talks to
    //! `HotkeyMonitor` directly through its own mpsc channel. The
    //! audio path IS the real FRB pipeline.
    //!
    //! Needs a microphone, the Whisper model (see
    //! `crate::model::default_search_dirs`) and a human holding
    //! Ctrl+Win. Run focused: it pre-warms the process-wide AUDIO
    //! static, which would break the guard-rail unit tests above if
    //! they ran afterwards in the same process:
    //!
    //! ```text
    //! cargo test --lib manual_dictation_demo -- --ignored --nocapture
    //! ```
    //!
    //! Language: set BREEZE_DEMO_LANG=en for English (default es).
    //!
    //! Injection (v3, FASE 3): set BREEZE_DEMO_INJECT=1 to paste
    //! each transcription into the focused window through the real
    //! `inject_text` api (full FR-05 sequence). Focus the target app
    //! (e.g. Notepad) while dictating; the console keeps echoing every
    //! transcription as a backup.

    use super::*;
    use crate::api::transcription::init_engine;
    use std::time::{Duration, Instant};

    const DICTATIONS: u32 = 2;
    /// How long the user has to press Ctrl+Win before a dictation is
    /// skipped.
    const COMBO_TIMEOUT: Duration = Duration::from_secs(60);
    /// Release wait: longer than the 60 s capture cap (FR-02) so a
    /// maxed-out dictation still completes normally.
    const RELEASE_TIMEOUT: Duration = Duration::from_secs(90);

    /// Waits for the next `ComboPressed`, skipping unrelated events.
    /// Returns false on timeout or channel loss.
    fn wait_combo_pressed(rx: &mpsc::Receiver<HotkeyEvent>, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match rx.recv_timeout(remaining) {
                Ok(HotkeyEvent::ComboPressed) => return true,
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    }

    /// Waits for the `ComboReleased` that ends the current dictation
    /// and returns its hold duration in ms. `None` on timeout or
    /// channel loss.
    fn wait_combo_released(rx: &mpsc::Receiver<HotkeyEvent>, timeout: Duration) -> Option<u64> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match rx.recv_timeout(remaining) {
                Ok(HotkeyEvent::ComboReleased { hold_ms, .. }) => return Some(hold_ms),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }

    #[test]
    #[ignore = "manual demo: needs mic, model and a human holding Ctrl+Win"]
    fn manual_dictation_demo() {
        // Console output is Spanish on purpose: the user reads it live
        // while dictating (project rule: code English, UX Spanish).
        let lang = match std::env::var("BREEZE_DEMO_LANG").as_deref() {
            Ok("en") => TranscriptionLanguage::En,
            _ => TranscriptionLanguage::Es,
        };

        // v3 - opt-in real paste into whatever window has focus.
        let inject_mode = std::env::var("BREEZE_DEMO_INJECT").as_deref() == Ok("1");

        println!();
        println!("=== BREEZE - DEMO DE DICTADO MANUAL ===");
        println!(
            "Idioma: {} (cambia con BREEZE_DEMO_LANG=es|en)",
            match lang {
                TranscriptionLanguage::Es => "espanol",
                TranscriptionLanguage::En => "ingles",
            }
        );

        if inject_mode {
            println!("Modo INYECCION activo (BREEZE_DEMO_INJECT=1).");
        }
        println!("Cargando el motor Whisper (unos segundos)...");
        let model_path = init_engine(false).expect("init_engine should locate and load the model");
        println!("Motor listo. Modelo: {model_path}");

        audio_prewarm().expect("audio_prewarm should find the default microphone");
        println!("Microfono pre-calentado.");

        let (tx, rx) = mpsc::channel();
        let monitor = HotkeyMonitor::start(tx).expect("hotkey monitor should start");
        // Let the hook thread and the WASAPI stream settle.
        std::thread::sleep(Duration::from_millis(300));

        for n in 1..=DICTATIONS {
            println!();
            if inject_mode {
                println!(
                    "===> Pon el FOCO en la app destino (ej. Notepad). Manten Ctrl+Win y HABLA. Al soltar, el texto se pegara donde este el cursor. (dictado {n} de {DICTATIONS})"
                );
            } else {
                println!(
                    "===> Manten Ctrl+Win y HABLA. Suelta para transcribir. (dictado {n} de {DICTATIONS})"
                );
            }
            if !wait_combo_pressed(&rx, COMBO_TIMEOUT) {
                println!(
                    "Sin Ctrl+Win en {} s: dictado {n} omitido.",
                    COMBO_TIMEOUT.as_secs()
                );
                continue;
            }

            if let Err(e) = audio_start_capture() {
                println!("No se pudo iniciar la captura ({e}): dictado {n} omitido.");
                continue;
            }
            println!("Grabando... suelta Ctrl+Win para terminar.");

            let Some(hold_ms) = wait_combo_released(&rx, RELEASE_TIMEOUT) else {
                // Discard the orphaned buffer so the next round starts
                // clean.
                let _ = audio_stop_capture_len();
                println!("No llego la soltada del combo: dictado {n} omitido.");
                continue;
            };

            let t0 = Instant::now();
            match audio_stop_capture_and_transcribe(lang) {
                Ok(text) => {
                    let transcribe_ms = t0.elapsed().as_millis();
                    if inject_mode {
                        // Timing - transcription takes ~1-2 s in a debug
                        // build, so the user has normally finished
                        // releasing Ctrl+Win by now; inject() still runs
                        // wait_for_modifiers_released internally to cover
                        // slow releases, so nothing extra is needed here.
                        match inject_text(text.clone()) {
                            Ok(outcome) => {
                                println!("INYECTADO ({outcome:?}) - revisa la app destino.")
                            }
                            Err(e) => println!("La inyeccion del dictado {n} fallo: {e}"),
                        }
                    }
                    // Always echo the transcription as a console backup.
                    println!("TRANSCRIPCION {n}: {text}");
                    println!(
                        "(audio: combo sostenido {hold_ms} ms; transcripcion: {transcribe_ms} ms)"
                    );
                }
                Err(e) => println!("La transcripcion del dictado {n} fallo: {e}"),
            }
        }

        println!();
        println!("DEMO COMPLETA");
        monitor.stop();
    }
}
