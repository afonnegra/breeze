//! Orchestrator runtime (FASE 4, Task 2).
//!
//! Owns the dedicated "orchestrator" thread that drives the pure
//! [`super::logic::OrchestratorLogic`] state machine against the real
//! modules (audio capture, whisper engine, injector) through an
//! injectable [`OrchestratorDeps`] bundle, so the whole runtime is
//! unit-testable with fakes (no GPU, no microphone, no hook).
//!
//! # Loop design (plan FASE 4, decisions 1-4)
//!
//! - The loop blocks on `hotkey_rx.recv_timeout(250 ms)`. An event is
//!   mapped to a logic [`Input`]; a timeout while Recording polls
//!   `is_truncated()` and feeds [`Input::CapReached`] on a hit (TD-007).
//! - `StopCaptureAndTranscribe` runs the engine on a per-cycle worker
//!   thread and waits on an internal channel guarded by a 15 s watchdog
//!   (decision 2). While waiting, hotkey events are NOT serviced; they
//!   queue up in `hotkey_rx` and are SELECTIVELY drained when the wait
//!   ends (TD-012): the stale release echoing this cycle is discarded,
//!   but if the user chained a new dictation and is still holding the
//!   combo (a trailing ComboPressed with no release), it is replayed so
//!   the next cycle starts Recording. A full press+release pair drained
//!   here is a dictation whose audio was never buffered - irrecoverable,
//!   so it is dropped with a warn.
//! - Watchdog expiry feeds `Input::TranscriptFailed("gpu timeout")`. A
//!   late engine result lands on a per-cycle channel whose receiver is
//!   already gone, so it is dropped by construction and can never paste
//!   into a later cycle.
//! - `Inject` runs synchronously on the orchestrator thread (~350 ms).
//! - NFR-12 - transcribed text is never logged; only lengths and
//!   per-phase durations (`cycle_id`, `phase`, `ms`) are, which is the
//!   raw material for the NFR-01 measurement in Task 4.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use super::logic::{Command, Input, OrchestratorLogic, State, UiState};
use crate::audio::convert::rms_energy;
use crate::hotkey::{HotkeyEvent, ReleaseReason};

/// Accidental-press threshold (FR-01.AC-2) fed to the pure logic.
pub const MIN_HOLD_MS: u64 = 200;

/// Transcription watchdog budget (plan decision 2). Production value;
/// tests inject a short one through [`OrchestratorDeps`].
pub const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Silence gate for TD-009. whisper hallucinates plausible text when
/// fed near-silence, so a capture whose normalized RMS energy (see
/// [`rms_energy`]) falls below this threshold is treated as an empty
/// transcript and never reaches the engine.
///
/// Calibration (2026-07-03, `calibrate_rms -- --ignored`, dev laptop
/// mic, ambient silence, 2 s per run, two runs) measured RMS 0.005362
/// and 0.005954. Threshold = max(3 x worst measurement, 0.005)
/// = 0.017863, rounded up to 0.018.
pub const SPEECH_RMS_THRESHOLD: f64 = 0.018;

/// Cap-poll cadence while Recording (TD-007); also bounds the shutdown
/// latency of [`OrchestratorHandle::stop`].
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Signature of the transcription dependency. It runs on a per-cycle
/// worker thread, so it must be shareable across threads.
pub type TranscribeFn = dyn Fn(Vec<i16>) -> Result<String, String> + Send + Sync;

/// Injectable side effects the runtime drives. Production wiring lives
/// in `crate::api::orchestrator::production_deps`; unit tests pass
/// fakes so the runtime is exercised without GPU, mic or hook.
pub struct OrchestratorDeps {
    /// Events from [`crate::hotkey::HotkeyMonitor`].
    pub hotkey_rx: Receiver<HotkeyEvent>,
    /// Begin buffering microphone audio.
    pub start_capture: Box<dyn Fn() + Send>,
    /// Stop buffering; returns the captured 16 kHz mono PCM.
    pub stop_capture: Box<dyn Fn() -> Vec<i16> + Send>,
    /// True when the capture in progress hit the 60 s cap (TD-007).
    pub is_truncated: Box<dyn Fn() -> bool + Send>,
    /// PCM to text. `Err` carries a displayable failure message.
    pub transcribe: Arc<TranscribeFn>,
    /// Paste text into the focused control. On success returns
    /// `(clipboard_restored, to_paste)` where `clipboard_restored`
    /// mirrors [`crate::inject::InjectionOutcome`] and `to_paste` is the
    /// time the injector spent from its own start up to the Ctrl+V
    /// dispatch (paste-landed), EXCLUDING the post-paste settle +
    /// clipboard restore. It is the NFR-01 measurement input.
    pub inject: Box<dyn Fn(&str) -> Result<(bool, Duration), String> + Send>,
    /// Overlay state stream toward the UI bridge.
    pub ui_tx: Sender<UiState>,
    /// Watchdog budget for one transcription.
    pub transcribe_timeout: Duration,
}

/// Errors starting the orchestrator runtime.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// The dedicated thread could not be spawned.
    #[error("orchestrator thread failed to start: {0}")]
    ThreadStartFailed(String),
}

/// Entry point for the orchestrator runtime.
pub struct Orchestrator;

impl Orchestrator {
    /// Spawn the dedicated "orchestrator" thread around `deps`.
    pub fn start(deps: OrchestratorDeps) -> Result<OrchestratorHandle, OrchestratorError> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = std::thread::Builder::new()
            .name("orchestrator".into())
            .spawn(move || Runtime::new(deps, thread_stop).run())
            .map_err(|e| OrchestratorError::ThreadStartFailed(e.to_string()))?;
        Ok(OrchestratorHandle {
            stop,
            join: Some(join),
        })
    }
}

/// Handle owning the orchestrator thread. Stops it on [`Self::stop`]
/// or on drop.
pub struct OrchestratorHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl OrchestratorHandle {
    /// Signal shutdown and join the thread. The loop notices the flag
    /// within one poll interval (or one watchdog slice while a
    /// transcription wait is in progress).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        self.stop.store(true, Ordering::Release);
        if join.join().is_err() {
            warn!("orchestrator thread panicked");
        }
    }
}

impl Drop for OrchestratorHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Thread-side state of the runtime loop.
struct Runtime {
    logic: OrchestratorLogic,
    deps: OrchestratorDeps,
    stop: Arc<AtomicBool>,
    /// Incremented on every StartCapture; correlates the phase logs of
    /// one dictation (NFR-01 instrumentation groundwork for Task 4).
    cycle_id: u64,
    /// Instant of the keyup (or cap cut) that closed the capture; the
    /// NFR-01 anchor for the end-to-end cycle duration.
    released_at: Option<Instant>,
    /// A chained ComboPressed rescued by the selective drain (TD-012),
    /// replayed after the current cycle settles.
    pending_replay: Option<Input>,
}

impl Runtime {
    fn new(deps: OrchestratorDeps, stop: Arc<AtomicBool>) -> Self {
        Self {
            logic: OrchestratorLogic::new(MIN_HOLD_MS),
            deps,
            stop,
            cycle_id: 0,
            released_at: None,
            pending_replay: None,
        }
    }

    fn run(mut self) {
        info!("orchestrator thread started");
        loop {
            if self.stop.load(Ordering::Acquire) {
                break;
            }
            match self.deps.hotkey_rx.recv_timeout(POLL_INTERVAL) {
                Ok(event) => {
                    if let Some(input) = map_event(event) {
                        self.feed(input);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.logic.state() == State::Recording && (self.deps.is_truncated)() {
                        info!(cycle_id = self.cycle_id, "cap hit, cutting proactively (TD-007)");
                        self.feed(Input::CapReached);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    warn!("hotkey channel closed; orchestrator exiting");
                    break;
                }
            }
            // Replay a chained ComboPressed rescued by the selective drain
            // (TD-012) after ANY feed settles - including the CapReached cut,
            // which also runs the transcription wait + drain. replay_pending
            // take()s the slot, so a no-op when nothing is pending; a slot is
            // only ever set once per wait, so no double replay.
            self.replay_pending();
        }
        info!("orchestrator thread stopped");
    }

    /// Feed one input to the pure logic and execute the resulting
    /// command cascade. Commands that produce a follow-up input
    /// (transcription result, injection outcome) are fed back until
    /// the machine settles.
    fn feed(&mut self, input: Input) {
        let mut pending: VecDeque<Command> = self.logic.handle(input).into();
        while let Some(command) = pending.pop_front() {
            if let Some(next) = self.execute(command) {
                pending.extend(self.logic.handle(next));
            }
        }
    }

    /// Replays a chained ComboPressed that arrived while the engine was
    /// busy and was set aside by the selective drain (TD-012). Called
    /// after a cycle command cascade settles so the new dictation starts
    /// a fresh Recording cycle instead of being lost.
    fn replay_pending(&mut self) {
        if let Some(input) = self.pending_replay.take() {
            info!("re-injecting chained ComboPressed drained during transcription (TD-012)");
            self.feed(input);
        }
    }

    /// Execute one side effect; returns the follow-up input, if any.
    fn execute(&mut self, command: Command) -> Option<Input> {
        match command {
            Command::StartCapture => {
                self.cycle_id += 1;
                self.released_at = None;
                (self.deps.start_capture)();
                info!(cycle_id = self.cycle_id, phase = "capture_start", "CyclePhase");
                None
            }
            Command::StopCaptureAndTranscribe => {
                let released = Instant::now();
                self.released_at = Some(released);
                let pcm = (self.deps.stop_capture)();
                let rms = rms_energy(&pcm);
                // 16 kHz mono PCM - 16 samples per millisecond.
                info!(
                    cycle_id = self.cycle_id,
                    phase = "capture_stop",
                    audio_ms = (pcm.len() / 16) as u64,
                    rms,
                    "CyclePhase"
                );
                if rms < SPEECH_RMS_THRESHOLD {
                    // TD-009 silence gate - feeding whisper (near-)silence
                    // makes it hallucinate text that would get pasted. A
                    // below-threshold capture is reported as an empty
                    // transcript, the exact path the pure logic already
                    // handles (FR-04 - cycle ends Hidden, no paste). Only
                    // numbers are logged, never content (NFR-12).
                    info!(
                        cycle_id = self.cycle_id,
                        rms,
                        threshold = SPEECH_RMS_THRESHOLD,
                        "silence gate hit - discarding capture as empty transcript (TD-009)"
                    );
                    return Some(Input::TranscriptReady(String::new()));
                }
                Some(self.transcribe_with_watchdog(pcm))
            }
            Command::DiscardCapture => {
                let pcm = (self.deps.stop_capture)();
                info!(
                    cycle_id = self.cycle_id,
                    phase = "discard",
                    audio_ms = (pcm.len() / 16) as u64,
                    "CyclePhase"
                );
                None
            }
            Command::Inject(text) => Some(self.run_inject(&text)),
            Command::EmitState(state) => {
                if self.deps.ui_tx.send(state).is_err() {
                    debug!("ui channel closed; overlay state dropped");
                }
                None
            }
        }
    }

    /// Run the engine on a per-cycle worker thread and wait for its
    /// result under the watchdog (plan decision 2). Hotkey events that
    /// queue up during the wait are drained afterwards (accepted
    /// simplification, module docs). A result arriving after the
    /// watchdog fired is dropped by construction - the per-cycle
    /// receiver is gone by then, so the worker send simply fails.
    fn transcribe_with_watchdog(&mut self, pcm: Vec<i16>) -> Input {
        let started = Instant::now();
        let (result_tx, result_rx) = mpsc::channel::<Result<String, String>>();
        let transcribe = Arc::clone(&self.deps.transcribe);
        let cycle_id = self.cycle_id;
        let spawned = std::thread::Builder::new()
            .name(format!("transcribe-{cycle_id}"))
            .spawn(move || {
                // A failed send only means the watchdog already fired
                // and the receiver is gone - the late result must die
                // here (the pure logic would ignore it anyway).
                let _ = result_tx.send(transcribe(pcm));
            });
        if let Err(e) = spawned {
            warn!(cycle_id, "transcription worker spawn failed - {e}");
            return Input::TranscriptFailed(format!("transcription worker spawn failed - {e}"));
        }

        let deadline = started + self.deps.transcribe_timeout;
        let input = loop {
            if self.stop.load(Ordering::Acquire) {
                // Shutting down - abandon the wait. The state reset is
                // irrelevant, the main loop exits right after.
                break Input::TranscriptFailed("orchestrator shutting down".into());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                warn!(cycle_id, "transcription watchdog expired");
                break Input::TranscriptFailed("gpu timeout".into());
            }
            match result_rx.recv_timeout(remaining.min(POLL_INTERVAL)) {
                Ok(Ok(text)) => {
                    info!(
                        cycle_id,
                        phase = "transcribe",
                        ms = started.elapsed().as_millis() as u64,
                        text_chars = text.chars().count(),
                        "CyclePhase"
                    );
                    break Input::TranscriptReady(text);
                }
                Ok(Err(message)) => {
                    warn!(cycle_id, "transcription failed - {message}");
                    break Input::TranscriptFailed(message);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    warn!(cycle_id, "transcription worker died without a result");
                    break Input::TranscriptFailed("transcription worker died".into());
                }
            }
        };
        // Selectively drain hotkey events accumulated during the blocking
        // wait (TD-012). The stale ComboReleased echoing this cycle is
        // discarded, but a chained dictation the user started while the
        // engine was busy must not be swallowed: if the drain ends on a
        // ComboPressed with no matching release, the user is still holding
        // the combo for a NEW dictation, so it is queued for replay once
        // this cycle settles. A full press+release pair drained here is a
        // dictation whose audio was never buffered (the mic was not
        // capturing) - irrecoverable, so it is dropped with a warn.
        let mut pending_press = false;
        let mut dropped_dictations: u32 = 0;
        while let Ok(event) = self.deps.hotkey_rx.try_recv() {
            match map_event(event) {
                Some(Input::ComboPressed) => pending_press = true,
                Some(Input::ComboReleased { .. }) => {
                    if pending_press {
                        pending_press = false;
                        dropped_dictations += 1;
                    }
                }
                Some(Input::SessionLocked) => pending_press = false,
                _ => {}
            }
        }
        if dropped_dictations > 0 {
            warn!(
                cycle_id,
                dropped_dictations,
                "chained dictation(s) drained during transcription wait - audio was not buffered, dropped (TD-012)"
            );
        }
        if pending_press {
            // A new dictation is mid-press; start it once this cycle ends.
            self.pending_replay = Some(Input::ComboPressed);
        }
        input
    }

    /// Synchronous injection on the orchestrator thread (~350 ms).
    /// NFR-12 - only the text length is logged, never the content.
    fn run_inject(&mut self, text: &str) -> Input {
        let started = Instant::now();
        let cycle_id = self.cycle_id;
        match (self.deps.inject)(text) {
            Ok((restored, to_paste)) => {
                info!(
                    cycle_id,
                    phase = "inject",
                    ms = started.elapsed().as_millis() as u64,
                    text_len = text.len(),
                    "CyclePhase"
                );
                if let Some(released) = self.released_at {
                    // NFR-01 - the requirement is "keyup -> text complete
                    // in the target input", i.e. the paste-landed moment,
                    // NOT the end of the cycle. paste_landed folds two
                    // spans: released -> injector entry (queue + the
                    // orchestrator getting to Inject) plus the injector's
                    // own time up to the Ctrl+V dispatch (to_paste). The
                    // post-paste settle + clipboard restore are excluded
                    // by construction (to_paste stops at the dispatch).
                    let to_injector = started.saturating_duration_since(released);
                    let paste_landed = to_injector + to_paste;
                    info!(
                        cycle_id,
                        phase = "paste_landed",
                        ms = paste_landed.as_millis() as u64,
                        "CyclePhase"
                    );
                    // Kept as a secondary, informational figure: the full
                    // cycle including settle + restore (keyup -> Hidden).
                    info!(
                        cycle_id,
                        phase = "cycle_total",
                        ms = released.elapsed().as_millis() as u64,
                        "CyclePhase"
                    );
                }
                Input::InjectFinished(restored)
            }
            Err(message) => {
                warn!(cycle_id, "injection failed - {message}");
                Input::InjectFailed(message)
            }
        }
    }
}

/// Map a monitor event onto a logic input. A release whose reason is
/// the session lock becomes [`Input::SessionLocked`] so Recording
/// discards instead of transcribing - the lock event that follows it
/// is then a no-op in Idle. `SessionUnlocked` has no logic input.
fn map_event(event: HotkeyEvent) -> Option<Input> {
    match event {
        HotkeyEvent::ComboPressed => Some(Input::ComboPressed),
        HotkeyEvent::ComboReleased {
            reason: ReleaseReason::SessionLocked,
            ..
        } => Some(Input::SessionLocked),
        HotkeyEvent::ComboReleased { hold_ms, .. } => Some(Input::ComboReleased { hold_ms }),
        HotkeyEvent::SessionLocked => Some(Input::SessionLocked),
        HotkeyEvent::SessionUnlocked => None,
    }
}

#[cfg(test)]
mod tests {
    //! Runtime unit tests with fake deps - no GPU, mic or hook.

    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    /// Call counters shared with the fake deps.
    #[derive(Default)]
    struct Counters {
        capture_starts: AtomicUsize,
        capture_stops: AtomicUsize,
        transcribe_calls: AtomicUsize,
        injected: Mutex<Vec<String>>,
    }

    impl Counters {
        fn injected_texts(&self) -> Vec<String> {
            self.injected
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    /// Running runtime wired to fakes, plus the test-side endpoints.
    struct Fixture {
        hotkey_tx: Sender<HotkeyEvent>,
        ui_rx: Receiver<UiState>,
        counters: Arc<Counters>,
        truncated: Arc<AtomicBool>,
        handle: Option<OrchestratorHandle>,
    }

    impl Fixture {
        fn press(&self) {
            self.hotkey_tx
                .send(HotkeyEvent::ComboPressed)
                .expect("send press");
        }

        fn release(&self, hold_ms: u64) {
            self.hotkey_tx
                .send(HotkeyEvent::ComboReleased {
                    hold_ms,
                    reason: ReleaseReason::KeyLifted,
                })
                .expect("send release");
        }

        fn expect_state(&self, what: &str) -> UiState {
            self.ui_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|e| panic!("timed out waiting for {what} - {e}"))
        }

        fn stop(&mut self) {
            if let Some(handle) = self.handle.take() {
                handle.stop();
            }
        }
    }

    /// PCM the fake capture returns by default - a constant well above
    /// [`SPEECH_RMS_THRESHOLD`] (8000/32768 = RMS 0.244) so ordinary
    /// tests pass the TD-009 silence gate.
    fn loud_pcm() -> Vec<i16> {
        vec![8_000i16; 1_600]
    }

    /// Build and start a runtime around fakes. `transcribe` and
    /// `inject_result` are the two behaviors the tests vary.
    fn start_fixture(
        transcribe: impl Fn(Vec<i16>) -> Result<String, String> + Send + Sync + 'static,
        inject_result: Result<bool, String>,
        timeout: Duration,
    ) -> Fixture {
        start_fixture_with_pcm(transcribe, inject_result, timeout, loud_pcm())
    }

    /// [`start_fixture`] with an explicit captured-PCM payload; the
    /// TD-009 silence-gate test passes silence here.
    fn start_fixture_with_pcm(
        transcribe: impl Fn(Vec<i16>) -> Result<String, String> + Send + Sync + 'static,
        inject_result: Result<bool, String>,
        timeout: Duration,
        pcm: Vec<i16>,
    ) -> Fixture {
        let (hotkey_tx, hotkey_rx) = mpsc::channel();
        let (ui_tx, ui_rx) = mpsc::channel();
        let counters = Arc::new(Counters::default());
        let truncated = Arc::new(AtomicBool::new(false));

        let c_start = Arc::clone(&counters);
        let c_stop = Arc::clone(&counters);
        let c_transcribe = Arc::clone(&counters);
        let c_inject = Arc::clone(&counters);
        let truncated_dep = Arc::clone(&truncated);

        let deps = OrchestratorDeps {
            hotkey_rx,
            start_capture: Box::new(move || {
                c_start.capture_starts.fetch_add(1, Ordering::SeqCst);
            }),
            stop_capture: Box::new(move || {
                c_stop.capture_stops.fetch_add(1, Ordering::SeqCst);
                pcm.clone()
            }),
            is_truncated: Box::new(move || truncated_dep.load(Ordering::SeqCst)),
            transcribe: Arc::new(move |pcm| {
                c_transcribe.transcribe_calls.fetch_add(1, Ordering::SeqCst);
                transcribe(pcm)
            }),
            inject: Box::new(move |text| {
                c_inject
                    .injected
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(text.to_owned());
                // Fake paste-landed timing - the unit tests only assert
                // on states/counters, not on the duration, so a zero is
                // enough to satisfy the (bool, Duration) contract.
                inject_result.clone().map(|restored| (restored, Duration::ZERO))
            }),
            ui_tx,
            transcribe_timeout: timeout,
        };
        let handle = Orchestrator::start(deps).expect("runtime should start");
        Fixture {
            hotkey_tx,
            ui_rx,
            counters,
            truncated,
            handle: Some(handle),
        }
    }

    #[test]
    fn happy_cycle_emits_states_in_order_and_injects() {
        let mut fx = start_fixture(|_| Ok("hola mundo".into()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(1_500);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.capture_starts.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.capture_stops.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.transcribe_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.injected_texts(), vec!["hola mundo".to_owned()]);
    }

    // FR-01.AC-2 - a short press is discarded without transcribing.
    #[test]
    fn accidental_press_discards_without_transcribing() {
        let mut fx = start_fixture(|_| Ok("nunca".into()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(MIN_HOLD_MS - 1);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.capture_stops.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.transcribe_calls.load(Ordering::SeqCst), 0);
        assert!(fx.counters.injected_texts().is_empty());
    }

    // Plan decision 2 - the watchdog cuts a hung engine and the late
    // result is dropped (no extra UI states, no paste).
    #[test]
    fn watchdog_expires_and_late_result_is_discarded() {
        let mut fx = start_fixture(
            |_| {
                std::thread::sleep(Duration::from_millis(600));
                Ok("tarde".into())
            },
            Ok(true),
            Duration::from_millis(100),
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(1_000);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        match fx.expect_state("Error") {
            UiState::Error(message) => assert!(
                message.contains("gpu timeout"),
                "unexpected watchdog message - {message}"
            ),
            other => panic!("expected Error state, got {other:?}"),
        }
        // Give the late result time to land (and be dropped).
        std::thread::sleep(Duration::from_millis(700));
        assert!(
            fx.ui_rx.try_recv().is_err(),
            "late transcription result must not produce UI states"
        );
        assert!(fx.counters.injected_texts().is_empty());
        fx.stop();
    }

    // TD-007 - the cap poll cuts the cycle without waiting for release.
    #[test]
    fn cap_reached_cuts_proactively_and_completes_cycle() {
        let mut fx = start_fixture(|_| Ok("texto largo".into()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.truncated.store(true, Ordering::SeqCst);
        // No release is sent - the 250 ms poll must pick up the cap.
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.injected_texts(), vec!["texto largo".to_owned()]);
    }

    // TD-009 - a (near-)silent capture is gated on RMS before the
    // engine. transcribe is never called, nothing is pasted, and the
    // cycle ends through the empty-transcript path (Hidden).
    #[test]
    fn silent_capture_is_gated_without_transcribing() {
        let mut fx = start_fixture_with_pcm(
            |_| Ok("hallucinated".into()),
            Ok(true),
            TRANSCRIBE_TIMEOUT,
            vec![0i16; 16_000],
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(1_000);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.capture_stops.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.transcribe_calls.load(Ordering::SeqCst), 0);
        assert!(fx.counters.injected_texts().is_empty());
    }

    // FR-04 - an empty transcript ends the cycle with no paste.
    #[test]
    fn empty_transcript_ends_cycle_without_paste() {
        let mut fx = start_fixture(|_| Ok(String::new()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(800);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert!(fx.counters.injected_texts().is_empty());
    }

    #[test]
    fn inject_failure_surfaces_error_state() {
        let mut fx = start_fixture(
            |_| Ok("algo".into()),
            Err("no focused control".into()),
            TRANSCRIBE_TIMEOUT,
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(500);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        match fx.expect_state("Error") {
            UiState::Error(message) => assert!(message.contains("no focused control")),
            other => panic!("expected Error state, got {other:?}"),
        }
        fx.stop();
    }

    // A release carrying the session-lock reason discards the capture
    // (map_event routes it to Input::SessionLocked).
    #[test]
    fn session_lock_release_discards_capture() {
        let mut fx = start_fixture(|_| Ok("nunca".into()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.hotkey_tx
            .send(HotkeyEvent::ComboReleased {
                hold_ms: 5_000,
                reason: ReleaseReason::SessionLocked,
            })
            .expect("send lock release");
        fx.hotkey_tx
            .send(HotkeyEvent::SessionLocked)
            .expect("send lock");
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.transcribe_calls.load(Ordering::SeqCst), 0);
        assert!(fx.counters.injected_texts().is_empty());
    }

    // Runtime-level recovery - after a failed transcription the next
    // dictation runs a clean, complete cycle.
    #[test]
    fn cycle_after_failed_transcription_starts_clean() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in = Arc::clone(&calls);
        let mut fx = start_fixture(
            move |_| {
                if calls_in.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("boom".into())
                } else {
                    Ok("segundo".into())
                }
            },
            Ok(true),
            TRANSCRIBE_TIMEOUT,
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(500);
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert!(matches!(fx.expect_state("Error"), UiState::Error(_)));
        fx.press();
        assert_eq!(fx.expect_state("Listening 2"), UiState::Listening);
        fx.release(500);
        assert_eq!(fx.expect_state("Transcribing 2"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting 2"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden 2"), UiState::Hidden);
        fx.stop();
        assert_eq!(fx.counters.injected_texts(), vec!["segundo".to_owned()]);
    }

    #[test]
    fn stop_joins_cleanly_while_idle() {
        let mut fx = start_fixture(|_| Ok("x".into()), Ok(true), TRANSCRIBE_TIMEOUT);
        fx.stop();
        assert!(fx.ui_rx.try_recv().is_err(), "no states expected");
    }

    // Blocks the calling test until `flag` flips true (the fake engine
    // has entered transcribe and is parked on the barrier), so chained
    // hotkey events can be enqueued while the drain has not run yet.
    fn wait_flag(flag: &AtomicBool) {
        for _ in 0..300 {
            if flag.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("fake transcribe did not start in time");
    }

    // TD-012 - a ComboPressed chained while the engine was busy (still
    // held, no release) survives the selective drain and starts a fresh
    // Recording cycle after the current one settles. Deterministic: the
    // fake engine parks on a barrier so the chained press is provably in
    // the channel before the drain runs.
    #[test]
    fn chained_press_pending_after_drain_starts_new_cycle() {
        let started = Arc::new(AtomicBool::new(false));
        let proceed = Arc::new(std::sync::Barrier::new(2));
        let started_dep = Arc::clone(&started);
        let proceed_dep = Arc::clone(&proceed);
        let mut fx = start_fixture(
            move |_| {
                started_dep.store(true, Ordering::SeqCst);
                proceed_dep.wait();
                Ok("uno".into())
            },
            Ok(true),
            TRANSCRIBE_TIMEOUT,
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(1_000);
        wait_flag(&started);
        // The user starts a new dictation and keeps holding (no release).
        fx.press();
        proceed.wait();
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        // The rescued press replays -> a second Recording cycle begins.
        assert_eq!(fx.expect_state("Listening 2"), UiState::Listening);
        fx.stop();
        assert_eq!(fx.counters.capture_starts.load(Ordering::SeqCst), 2);
        assert_eq!(fx.counters.injected_texts(), vec!["uno".to_owned()]);
    }

    // TD-012 - a full press+release pair chained during the wait is a
    // dictation whose audio was never buffered; it is dropped (warn), no
    // replay, and no second cycle runs.
    #[test]
    fn chained_full_pair_drained_drops_dictation_and_stays_idle() {
        let started = Arc::new(AtomicBool::new(false));
        let proceed = Arc::new(std::sync::Barrier::new(2));
        let started_dep = Arc::clone(&started);
        let proceed_dep = Arc::clone(&proceed);
        let mut fx = start_fixture(
            move |_| {
                started_dep.store(true, Ordering::SeqCst);
                proceed_dep.wait();
                Ok("uno".into())
            },
            Ok(true),
            TRANSCRIBE_TIMEOUT,
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        fx.release(1_000);
        wait_flag(&started);
        // A complete chained dictation lands entirely inside the wait.
        fx.press();
        fx.release(1_000);
        proceed.wait();
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        fx.stop();
        // Exactly one transcription and one capture cycle - the drained
        // pair never became a cycle (no audio to transcribe), no replay.
        assert_eq!(fx.counters.transcribe_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.capture_starts.load(Ordering::SeqCst), 1);
        assert_eq!(fx.counters.injected_texts(), vec!["uno".to_owned()]);
    }

    // TD-012 x TD-007 - the cap cut (no release) also runs the
    // transcription wait + selective drain, so a ComboPressed chained
    // during that wait must be replayed just like the release path. The
    // replay_pending call was moved to the end of the loop body so this
    // holds for the cap branch too (review fix A-2). One replay only.
    #[test]
    fn chained_press_pending_after_cap_cut_starts_new_cycle() {
        let started = Arc::new(AtomicBool::new(false));
        let proceed = Arc::new(std::sync::Barrier::new(2));
        let started_dep = Arc::clone(&started);
        let proceed_dep = Arc::clone(&proceed);
        let mut fx = start_fixture(
            move |_| {
                started_dep.store(true, Ordering::SeqCst);
                proceed_dep.wait();
                Ok("uno".into())
            },
            Ok(true),
            TRANSCRIBE_TIMEOUT,
        );
        fx.press();
        assert_eq!(fx.expect_state("Listening"), UiState::Listening);
        // No release: the 60 s cap fires and the poll cuts the cycle.
        fx.truncated.store(true, Ordering::SeqCst);
        wait_flag(&started);
        // Clear the cap so the replayed second cycle is not cut again.
        fx.truncated.store(false, Ordering::SeqCst);
        // The user starts a NEW dictation and keeps holding (no release).
        fx.press();
        proceed.wait();
        assert_eq!(fx.expect_state("Transcribing"), UiState::Transcribing);
        assert_eq!(fx.expect_state("Injecting"), UiState::Injecting);
        assert_eq!(fx.expect_state("Hidden"), UiState::Hidden);
        // The rescued press replays after the cap cycle -> second Recording.
        assert_eq!(fx.expect_state("Listening 2"), UiState::Listening);
        fx.stop();
        assert_eq!(fx.counters.capture_starts.load(Ordering::SeqCst), 2);
        assert_eq!(fx.counters.injected_texts(), vec!["uno".to_owned()]);
    }
}
