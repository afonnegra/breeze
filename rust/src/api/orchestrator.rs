//! FRB orchestrator API (FASE 4, Task 2) - start/stop the resident
//! dictation orchestrator and stream overlay states to Dart.
//!
//! `start_orchestrator` wires the real modules together. It pre-warms
//! the process-wide [`crate::audio::AudioCapture`] (reusing an existing
//! one), starts the [`HotkeyMonitor`] on a private channel, spawns the
//! orchestrator runtime around the production dependencies and bridges
//! the UI state channel into the Dart `StreamSink`. The whisper engine
//! is NOT initialized here - the Flutter startup sequence calls
//! `init_engine` first (Task 3); an uninitialized engine surfaces as an
//! Error overlay state on the first dictation, never as a crash.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::api::input::{audio_prewarm, with_audio, InputError};
use crate::api::transcription::{get_language, transcribe_pcm};
use crate::audio::AudioCapture;
use crate::frb_generated::StreamSink;
use crate::hotkey::{HotkeyEvent, HotkeyMonitor, HotkeyMonitorHandle};
use crate::inject::{self, InjectionOutcome};
use crate::orchestrator::logic::UiState;
use crate::orchestrator::runtime::{
    Orchestrator, OrchestratorDeps, OrchestratorHandle, TranscribeFn, TRANSCRIBE_TIMEOUT,
};

/// Overlay state exposed to Dart (mirror of
/// [`crate::orchestrator::logic::UiState`], kept separate so the FRB
/// scanner only walks `api::` types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiStateDto {
    /// Recording is active (combo held).
    Listening,
    /// Audio was sent to the engine; waiting for text.
    Transcribing,
    /// Text is being pasted into the focused control.
    Injecting,
    /// A cycle failed; the message goes to the overlay and the log.
    Error { message: String },
    /// No cycle in flight; the overlay hides itself.
    Hidden,
}

impl From<UiState> for UiStateDto {
    fn from(state: UiState) -> Self {
        match state {
            UiState::Listening => Self::Listening,
            UiState::Transcribing => Self::Transcribing,
            UiState::Injecting => Self::Injecting,
            UiState::Error(message) => Self::Error { message },
            UiState::Hidden => Self::Hidden,
        }
    }
}

/// Running orchestrator stack - runtime thread, hotkey monitor and the
/// UI bridge thread pumping states into the Dart sink.
struct OrchestratorState {
    runtime: OrchestratorHandle,
    monitor: HotkeyMonitorHandle,
    ui_bridge: JoinHandle<()>,
}

/// Process-wide orchestrator slot. `Mutex<Option<...>>` (not OnceLock)
/// so stop/start cycles work (FR-11 pause/resume).
static ORCHESTRATOR: Mutex<Option<OrchestratorState>> = Mutex::new(None);

fn lock_orchestrator() -> std::sync::MutexGuard<'static, Option<OrchestratorState>> {
    // Poison recovery - the Option inside stays meaningful even if a
    // thread panicked while holding the lock.
    ORCHESTRATOR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Production dependency wiring - process-wide audio capture, resident
/// whisper engine with the global language, real injector. Shared with
/// the end-to-end integration test below.
pub(crate) fn production_deps(
    hotkey_rx: mpsc::Receiver<HotkeyEvent>,
    ui_tx: mpsc::Sender<UiState>,
    transcribe_timeout: Duration,
) -> OrchestratorDeps {
    let transcribe: Arc<TranscribeFn> =
        Arc::new(|pcm| transcribe_pcm(pcm, get_language()).map_err(|e| e.to_string()));
    OrchestratorDeps {
        hotkey_rx,
        start_capture: Box::new(|| {
            if with_audio(AudioCapture::start_buffer).is_none() {
                warn!("orchestrator start_capture - audio not pre-warmed");
            }
        }),
        stop_capture: Box::new(|| {
            with_audio(|capture| capture.stop_buffer().pcm).unwrap_or_else(|| {
                warn!("orchestrator stop_capture - audio not pre-warmed");
                Vec::new()
            })
        }),
        is_truncated: Box::new(|| with_audio(AudioCapture::is_truncated).unwrap_or(false)),
        transcribe,
        inject: Box::new(|text| {
            // FR-05 fallback - keep the backup even if the paste fails.
            inject::store_last_transcription(text);
            // inject_timed drives the exact same production sequence as
            // inject() (settle + restore intact); it only ALSO reports
            // the paste-landed duration the runtime uses for NFR-01.
            inject::inject_timed(text)
                .map(|timed| {
                    (
                        matches!(timed.outcome, InjectionOutcome::Pasted),
                        timed.to_paste,
                    )
                })
                .map_err(|e| e.to_string())
        }),
        ui_tx,
        transcribe_timeout,
    }
}

/// Start the resident dictation orchestrator and stream overlay states
/// to Dart. Pre-warms (or reuses) the process-wide audio capture and
/// starts the hotkey monitor internally; call `init_engine` before the
/// first dictation or the cycle will surface an Error state.
pub fn start_orchestrator(sink: StreamSink<UiStateDto>) -> Result<(), InputError> {
    let mut slot = lock_orchestrator();
    if slot.is_some() {
        warn!("start_orchestrator - already running");
        return Err(InputError::OrchestratorAlreadyRunning);
    }

    audio_prewarm()?;

    let (hotkey_tx, hotkey_rx) = mpsc::channel::<HotkeyEvent>();
    let monitor = HotkeyMonitor::start(hotkey_tx)
        .inspect_err(|e| warn!(error = %e, "start_orchestrator - monitor start failed"))?;

    let (ui_tx, ui_rx) = mpsc::channel::<UiState>();
    let ui_bridge = match std::thread::Builder::new()
        .name("orchestrator-ui-bridge".into())
        .spawn(move || {
            // Exits when the runtime drops its ui_tx (orchestrator stop).
            while let Ok(state) = ui_rx.recv() {
                debug!(?state, "UiState forwarded to Dart");
                if sink.add(UiStateDto::from(state)).is_err() {
                    // Dart listener is gone; keep draining so the
                    // runtime never blocks on a full channel.
                    debug!("ui bridge - sink closed, state dropped");
                }
            }
            debug!("ui bridge thread exiting (channel closed)");
        }) {
        Ok(join) => join,
        Err(e) => {
            monitor.stop();
            warn!(error = %e, "start_orchestrator - ui bridge spawn failed");
            return Err(InputError::OrchestratorStartFailed(e.to_string()));
        }
    };

    let deps = production_deps(hotkey_rx, ui_tx, TRANSCRIBE_TIMEOUT);
    let runtime = match Orchestrator::start(deps) {
        Ok(handle) => handle,
        Err(e) => {
            // deps (with its ui_tx) was consumed and dropped by the
            // failed start, so the bridge ends on its own - join it.
            monitor.stop();
            if ui_bridge.join().is_err() {
                warn!("start_orchestrator - ui bridge panicked during rollback");
            }
            warn!(error = %e, "start_orchestrator - runtime start failed");
            return Err(InputError::OrchestratorStartFailed(e.to_string()));
        }
    };

    *slot = Some(OrchestratorState {
        runtime,
        monitor,
        ui_bridge,
    });
    info!("start_orchestrator - orchestrator running, ui states streaming to Dart");
    Ok(())
}

/// Stop the orchestrator started by [`start_orchestrator`].
pub fn stop_orchestrator() -> Result<(), InputError> {
    let state = lock_orchestrator()
        .take()
        .ok_or(InputError::OrchestratorNotRunning)
        .inspect_err(|_| warn!("stop_orchestrator - no orchestrator running"))?;
    // Order matters - stopping the runtime drops its ui_tx, which ends
    // the bridge; stopping the monitor closes the hotkey channel.
    state.runtime.stop();
    state.monitor.stop();
    if state.ui_bridge.join().is_err() {
        warn!("stop_orchestrator - ui bridge thread panicked");
    }
    info!("stop_orchestrator - stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_dto_maps_all_variants() {
        assert_eq!(UiStateDto::from(UiState::Listening), UiStateDto::Listening);
        assert_eq!(
            UiStateDto::from(UiState::Transcribing),
            UiStateDto::Transcribing
        );
        assert_eq!(UiStateDto::from(UiState::Injecting), UiStateDto::Injecting);
        assert_eq!(
            UiStateDto::from(UiState::Error("boom".into())),
            UiStateDto::Error {
                message: "boom".into()
            }
        );
        assert_eq!(UiStateDto::from(UiState::Hidden), UiStateDto::Hidden);
    }

    #[test]
    fn stop_without_start_reports_not_running() {
        assert!(matches!(
            stop_orchestrator(),
            Err(InputError::OrchestratorNotRunning)
        ));
    }
}

#[cfg(test)]
mod integration_tests {
    //! End-to-end synthetic-real integration (plan FASE 4, Task 2) -
    //! real engine + mic + monitor + injector wired through
    //! [`production_deps`], driven by a synthetic Ctrl+Win WITHOUT the
    //! inject marker, pasting into a self-contained EDIT window. It
    //! injects real key events system-wide, so run focused and hands
    //! off the keyboard with
    //!
    //! ```text
    //! cargo test --lib orchestrator_end_to_end -- --ignored --nocapture --test-threads=1
    //! ```

    use super::*;
    use crate::api::transcription::init_engine;
    use std::mem::size_of;
    use std::sync::mpsc::{channel, Receiver};
    use std::thread;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetForegroundWindow, GetMessageW,
        GetWindowThreadProcessId, PostThreadMessageW, SendMessageW, SetForegroundWindow,
        TranslateMessage, MSG, WINDOW_EX_STYLE, WM_GETTEXT, WM_QUIT, WS_POPUP, WS_VISIBLE,
    };

    const VK_LCONTROL: u16 = 0xA2;
    const VK_LWIN: u16 = 0x5B;

    /// Synthetic UNMARKED key event - it must travel the real LL hook.
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
        // SAFETY: input is fully initialized and SendInput copies it.
        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        assert_eq!(sent, 1, "SendInput failed for vk {vk:#04X}");
    }

    /// Mandatory cleanup guard - releases the injected keys even if
    /// the test panics mid-combo, so Ctrl/Win never stay stuck.
    struct KeyCleanup;

    impl Drop for KeyCleanup {
        fn drop(&mut self) {
            for vk in [VK_LCONTROL, VK_LWIN] {
                send_key(vk, true);
            }
        }
    }

    /// Brings hwnd to the foreground with the AttachThreadInput
    /// workaround (background processes are denied SetForegroundWindow).
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

    /// Self-contained EDIT window with its own message pump thread
    /// (same pattern as the inject integration tests).
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
                // SAFETY: EDIT is a system window class; a visible popup
                // EDIT is a valid top-level window that accepts focus
                // and paste. Empty title = empty initial content.
                let hwnd = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        w!("EDIT"),
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
            let mut buf = [0u16; 1024];
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

    fn expect_state(rx: &Receiver<UiState>, what: &str, timeout: Duration) -> UiState {
        rx.recv_timeout(timeout)
            .unwrap_or_else(|e| panic!("timed out waiting for {what} - {e}"))
    }

    #[test]
    #[ignore = "end to end - needs mic, whisper model, GPU and an interactive desktop; run focused, --test-threads=1"]
    fn orchestrator_end_to_end() {
        println!("loading whisper engine (a few seconds)...");
        let model = init_engine(false).expect("init_engine should locate and load the model");
        println!("engine ready, model at {model}");
        audio_prewarm().expect("audio_prewarm should find a microphone");

        let (hotkey_tx, hotkey_rx) = channel();
        let monitor = HotkeyMonitor::start(hotkey_tx).expect("monitor should start");
        let (ui_tx, ui_rx) = channel();
        let runtime = Orchestrator::start(production_deps(hotkey_rx, ui_tx, TRANSCRIBE_TIMEOUT))
            .expect("runtime should start");
        // Let the hook thread and the WASAPI stream settle.
        thread::sleep(Duration::from_millis(300));

        let window = EditWindow::spawn();
        assert!(
            window.focused,
            "could not focus the EDIT window - rerun with the test terminal focused"
        );

        let cleanup = KeyCleanup;
        // Synthetic Ctrl+Win WITHOUT the inject marker, held 1.5 s; the
        // mic captures whatever ambient sound there is.
        send_key(VK_LCONTROL, false);
        send_key(VK_LWIN, false);
        thread::sleep(Duration::from_millis(1_500));
        send_key(VK_LWIN, true);
        send_key(VK_LCONTROL, true);
        drop(cleanup);

        assert_eq!(
            expect_state(&ui_rx, "Listening", Duration::from_secs(5)),
            UiState::Listening
        );
        assert_eq!(
            expect_state(&ui_rx, "Transcribing", Duration::from_secs(5)),
            UiState::Transcribing
        );
        // Ambient audio may transcribe to text (paste path) or to
        // nothing (silent cycle end) - both are valid results (FR-04).
        let pasted = match expect_state(&ui_rx, "Injecting or Hidden", Duration::from_secs(30)) {
            UiState::Injecting => {
                assert_eq!(
                    expect_state(&ui_rx, "Hidden", Duration::from_secs(15)),
                    UiState::Hidden
                );
                true
            }
            UiState::Hidden => false,
            UiState::Error(message) => panic!("cycle failed - {message}"),
            other => panic!("unexpected state after Transcribing - {other:?}"),
        };

        let text = window.text();
        println!();
        if pasted {
            // Test-only echo so the runner can report what whisper
            // heard; the app itself never logs content (NFR-12).
            println!("whisper heard and pasted -> {text:?}");
            assert!(
                !text.is_empty(),
                "Injecting was emitted but the EDIT stayed empty"
            );
        } else {
            println!("whisper heard nothing usable - empty transcript, no paste, cycle ended");
            assert!(
                text.is_empty(),
                "no paste expected but the EDIT contains {text:?}"
            );
        }
        assert!(
            ui_rx.try_recv().is_err(),
            "no further UI states expected after the cycle"
        );

        runtime.stop();
        monitor.stop();
        println!("END-TO-END OK (pasted = {pasted})");
    }

    /// NFR-01 end-to-end measurement harness (plan FASE 4, Task 4).
    /// Real whisper engine (GPU), real hotkey monitor driven by a
    /// synthetic unmarked Ctrl+Win, real clipboard injection into a
    /// self-contained EDIT window. The microphone dep is replaced by a
    /// fake capture returning the en-5s.wav fixture PCM (~4.8 s of real
    /// English speech) - a real mic would capture ambient silence and
    /// the TD-009 RMS gate would (correctly) discard every cycle. The
    /// deps are injectable precisely to allow this. Hold time only has
    /// to beat MIN_HOLD_MS - the measured audio comes from the fixture.
    ///
    /// The NFR-01 metric is paste_landed_ms per cycle: keyup until the
    /// Ctrl+V is dispatched into the target input, EXCLUDING the
    /// post-paste PASTE_SETTLE_MS and the clipboard restore (housekeeping
    /// the user never waits on). Also reports transcribe_ms / inject_ms
    /// and cycle_total_ms (keyup -> UiState::Hidden, incl. settle +
    /// restore) as informational context, plus p50/p95 of paste_landed.
    /// NFR-01 threshold for a ~5 s clip is 500 ms; a miss prints a RED
    /// FLAG but does not fail the test (plan Task 4). Run in release,
    /// focused, hands off the keyboard, with
    ///
    /// ```text
    /// cargo test --release --lib bench_nfr01_end_to_end -- --ignored --nocapture --test-threads=1
    /// ```
    #[test]
    #[ignore = "NFR-01 bench - needs GPU, model, fixture and an interactive desktop; run --release focused"]
    fn bench_nfr01_end_to_end() {
        use crate::api::transcription::{transcribe_pcm, TranscriptionLanguage};
        use std::time::Instant;

        const CYCLES: usize = 5;

        // Fixture PCM, same contract as the whisper_engine tests.
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("test-fixtures")
            .join("en-5s.wav");
        let mut reader = hound::WavReader::open(&fixture).expect("en-5s.wav fixture missing");
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000, "fixture must be 16 kHz");
        assert_eq!(spec.channels, 1, "fixture must be mono");
        let pcm: Vec<i16> = reader
            .samples::<i16>()
            .map(|s| s.expect("valid PCM sample"))
            .collect();
        let audio_secs = pcm.len() as f64 / 16_000.0;

        println!("loading whisper engine (a few seconds)...");
        init_engine(false).expect("init_engine should locate and load the model");
        // Warmup - the first transcription after load pays the CUDA
        // warmup (kernels + buffers) and must not pollute the cycles.
        let t0 = Instant::now();
        transcribe_pcm(pcm.clone(), TranscriptionLanguage::En).expect("warmup transcription");
        println!("warmup transcription (not measured) took {:?}", t0.elapsed());

        let transcribe_times = Arc::new(Mutex::new(Vec::<f64>::new()));
        let inject_times = Arc::new(Mutex::new(Vec::<f64>::new()));
        // Absolute instant the paste landed (Ctrl+V dispatched), one per
        // cycle in injection order. The loop, which owns the per-cycle
        // keyup t0, turns each into the NFR-01 paste_landed_ms.
        let paste_landed_marks = Arc::new(Mutex::new(Vec::<Instant>::new()));

        let (hotkey_tx, hotkey_rx) = channel();
        let monitor = HotkeyMonitor::start(hotkey_tx).expect("monitor should start");
        let (ui_tx, ui_rx) = channel();

        let t_acc = Arc::clone(&transcribe_times);
        let i_acc = Arc::clone(&inject_times);
        let pl_acc = Arc::clone(&paste_landed_marks);
        let fixture_pcm = pcm.clone();
        let deps = OrchestratorDeps {
            hotkey_rx,
            start_capture: Box::new(|| {}),
            stop_capture: Box::new(move || fixture_pcm.clone()),
            is_truncated: Box::new(|| false),
            transcribe: Arc::new(move |p| {
                let t0 = Instant::now();
                let r = transcribe_pcm(p, TranscriptionLanguage::En).map_err(|e| e.to_string());
                t_acc
                    .lock()
                    .expect("transcribe acc lock")
                    .push(t0.elapsed().as_secs_f64() * 1000.0);
                r
            }),
            inject: Box::new(move |text| {
                // Mirror of production_deps (FR-05 backup + real paste).
                let t0 = Instant::now();
                inject::store_last_transcription(text);
                let timed = inject::inject_timed(text);
                if let Ok(t) = &timed {
                    // Paste-landed = injector entry + its own time up to
                    // the Ctrl+V dispatch, EXCLUDING settle + restore.
                    pl_acc
                        .lock()
                        .expect("paste-landed acc lock")
                        .push(t0 + t.to_paste);
                }
                let r = timed
                    .map(|t| (matches!(t.outcome, InjectionOutcome::Pasted), t.to_paste))
                    .map_err(|e| e.to_string());
                // Full injector call time (settle + restore included),
                // kept as the informational inject_ms column.
                i_acc
                    .lock()
                    .expect("inject acc lock")
                    .push(t0.elapsed().as_secs_f64() * 1000.0);
                r
            }),
            ui_tx,
            transcribe_timeout: TRANSCRIBE_TIMEOUT,
        };
        let runtime = Orchestrator::start(deps).expect("runtime should start");
        thread::sleep(Duration::from_millis(300));

        let window = EditWindow::spawn();
        assert!(
            window.focused,
            "could not focus the EDIT window - rerun with the test terminal focused"
        );

        // Per-cycle keyup instant, used to turn each paste-landed mark
        // into a keyup -> paste_landed duration after the run.
        let mut keyup_marks: Vec<Instant> = Vec::with_capacity(CYCLES);
        let mut totals_ms: Vec<f64> = Vec::with_capacity(CYCLES);
        // Defense-in-depth over the settle-drain: wait for `wanted`,
        // tolerating stale Listening/Hidden that a phantom cycle (the
        // synthetic Ctrl+Win release seen by the real LL hook) can
        // interleave. Any OTHER unexpected state (Injecting out of turn,
        // Error) still fails loudly. This makes the documented cycle-5
        // "Hidden instead of Transcribing" race impossible regardless of
        // timing. A genuine Error surfaces as a panic with its message.
        let await_state = |rx: &Receiver<UiState>, wanted: UiState, timeout: Duration| {
            let deadline = Instant::now() + timeout;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let got = rx
                    .recv_timeout(remaining)
                    .unwrap_or_else(|e| panic!("timed out waiting for {wanted:?} - {e}"));
                if got == wanted {
                    return;
                }
                match got {
                    // Stale phantom-cycle states - skip and keep waiting.
                    UiState::Listening | UiState::Hidden => {
                        eprintln!("skipping stale {got:?} while awaiting {wanted:?}");
                    }
                    other => panic!("unexpected {other:?} while awaiting {wanted:?}"),
                }
            }
        };
        for cycle in 1..=CYCLES {
            let cleanup = KeyCleanup;
            send_key(VK_LCONTROL, false);
            send_key(VK_LWIN, false);
            await_state(&ui_rx, UiState::Listening, Duration::from_secs(5));
            // Beat MIN_HOLD_MS so the release is not discarded.
            thread::sleep(Duration::from_millis(400));
            let t0 = Instant::now();
            keyup_marks.push(t0);
            send_key(VK_LWIN, true);
            send_key(VK_LCONTROL, true);
            drop(cleanup);
            await_state(&ui_rx, UiState::Transcribing, Duration::from_secs(5));
            await_state(&ui_rx, UiState::Injecting, Duration::from_secs(20));
            await_state(&ui_rx, UiState::Hidden, Duration::from_secs(15));
            let total = t0.elapsed().as_secs_f64() * 1000.0;
            totals_ms.push(total);
            println!("cycle {cycle} keyup -> paste landed reported below (cycle total {total:.1} ms)");
            // Cycle stability fix (root cause of the intermittent
            // "left: Hidden, right: Transcribing" assert). The synthetic
            // Ctrl+Win chord the cleanup releases is seen by the real LL
            // hook as a brief press+release, which fires a PHANTOM
            // dictation cycle. Being a sub-MIN_HOLD_MS press it is
            // discarded (FR-01.AC-2), emitting an extra Listening->Hidden
            // AFTER this cycle's Hidden. If the next real cycle starts
            // before that phantom lands, its stray Hidden is read where
            // Transcribing is expected. A single drain does not help - the
            // phantom states arrive asynchronously over the following
            // moments. Fix: drain ui_rx in a timed settle loop long enough
            // for any phantom cycle to fully run and be flushed before the
            // next real press.
            let settle_deadline = Instant::now() + Duration::from_millis(700);
            while Instant::now() < settle_deadline {
                while ui_rx.try_recv().is_ok() {}
                thread::sleep(Duration::from_millis(25));
            }
            // Final sweep after the settle window in case a phantom state
            // landed on the very last tick.
            while ui_rx.try_recv().is_ok() {}
        }

        assert!(
            !window.text().is_empty(),
            "five injected cycles must leave text in the EDIT window"
        );
        runtime.stop();
        monitor.stop();

        let t_ms = transcribe_times.lock().expect("transcribe acc lock").clone();
        let i_ms = inject_times.lock().expect("inject acc lock").clone();
        let pl_marks = paste_landed_marks
            .lock()
            .expect("paste-landed acc lock")
            .clone();
        assert_eq!(t_ms.len(), CYCLES, "one transcription per cycle expected");
        assert_eq!(i_ms.len(), CYCLES, "one injection per cycle expected");
        assert_eq!(
            pl_marks.len(),
            CYCLES,
            "one paste-landed mark per cycle expected"
        );

        // NFR-01 metric: keyup -> paste landed, per cycle. Injection
        // order matches cycle order (cycles run strictly serially), so
        // the nth mark pairs with the nth keyup.
        let paste_landed_ms: Vec<f64> = keyup_marks
            .iter()
            .zip(pl_marks.iter())
            .map(|(keyup, landed)| landed.saturating_duration_since(*keyup).as_secs_f64() * 1000.0)
            .collect();

        // Percentile helper (nearest-rank on a sorted copy).
        let percentile = |data: &[f64], pct: f64| -> f64 {
            let mut sorted = data.to_vec();
            sorted.sort_by(|a, b| a.total_cmp(b));
            let rank = ((pct / 100.0) * sorted.len() as f64).ceil() as usize;
            let idx = rank.saturating_sub(1).min(sorted.len() - 1);
            sorted[idx]
        };
        let pl_p50 = percentile(&paste_landed_ms, 50.0);
        let pl_p95 = percentile(&paste_landed_ms, 95.0);
        let total_p50 = percentile(&totals_ms, 50.0);

        const NFR01_THRESHOLD_MS: f64 = 500.0;

        println!();
        println!(
            "NFR-01 end-to-end - fixture en-5s.wav ({audio_secs:.2} s of audio), {CYCLES} cycles"
        );
        println!("(paste_landed_ms = keyup -> Ctrl+V dispatched, excl. settle+restore;");
        println!(" cycle_total_ms = keyup -> Hidden, incl. settle+restore, informational)");
        println!();
        println!("| cycle | paste_landed_ms | transcribe_ms | inject_ms | cycle_total_ms |");
        println!("|-------|-----------------|---------------|-----------|----------------|");
        for c in 0..CYCLES {
            println!(
                "| {:>5} | {:>15.1} | {:>13.1} | {:>9.1} | {:>14.1} |",
                c + 1,
                paste_landed_ms[c],
                t_ms[c],
                i_ms[c],
                totals_ms[c]
            );
        }
        println!();
        println!(
            "paste_landed p50 = {pl_p50:.1} ms | p95 = {pl_p95:.1} ms (NFR-01 threshold {NFR01_THRESHOLD_MS:.0} ms for a ~5 s clip)"
        );
        println!("cycle_total p50 = {total_p50:.1} ms (informational; includes settle+restore)");
        if pl_p50 <= NFR01_THRESHOLD_MS {
            println!("NFR-01 verdict: PASS (paste_landed p50 within threshold)");
        } else {
            // Plan Task 4 - a miss is a documented red flag, not a
            // test failure.
            println!("NFR-01 verdict: RED FLAG - paste_landed p50 exceeds threshold, document in phase evidence");
        }
    }
}
