//! Pure dictation state machine (FASE 4, Task 1).
//!
//! `OrchestratorLogic` is 100% pure. There is no I/O, no threading and
//! no clock in here; the accidental-press decision (FR-01.AC-2) uses
//! the `hold_ms` value measured by the runtime and carried inside
//! [`Input::ComboReleased`]. Every `handle` call maps (current state,
//! input) to a new state plus the ordered list of [`Command`]s the
//! runtime must execute.
//!
//! # Robustness decisions
//!
//! - `ComboPressed` while already active (Recording, Transcribing or
//!   Injecting) is ignored; the cycle in flight is never re-entered.
//! - `ComboReleased` outside Recording is ignored. In particular the
//!   physical release that follows a proactive cap cut (TD-007)
//!   arrives while Transcribing and must not disturb the cycle.
//! - `TranscriptReady` and `TranscriptFailed` outside Transcribing are
//!   ignored. After a watchdog timeout the runtime has already moved
//!   on, so a late engine result must be dropped, never pasted.
//! - `CapReached` outside Recording is ignored (stale poll result).
//! - `SessionLocked` only aborts while Recording. While Transcribing
//!   or Injecting the cycle continues, because the audio was captured
//!   before the lock and the pure logic cannot know whether the
//!   session will be unlocked again by paste time. If it is still
//!   locked, the paste lands nowhere or fails, which is harmless; the
//!   runtime stays free to add a stricter policy later.

/// UI-facing overlay state (FR-03), emitted via [`Command::EmitState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiState {
    /// Recording is active (combo held).
    Listening,
    /// Audio was sent to the engine; waiting for text.
    Transcribing,
    /// Text is being pasted into the focused control.
    Injecting,
    /// A cycle failed; the message goes to the overlay and the log.
    Error(String),
    /// No cycle in flight; the overlay hides itself.
    Hidden,
}

/// Orchestrator lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Waiting for the hotkey combo.
    Idle,
    /// Combo held; audio is being captured.
    Recording,
    /// Capture stopped; whisper is producing text.
    Transcribing,
    /// Transcript handed to the injector.
    Injecting,
}

/// External events fed into the state machine by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// The Ctrl+Win combo went down.
    ComboPressed,
    /// The combo was released after being held for `hold_ms`.
    ComboReleased { hold_ms: u64 },
    /// The Windows session is locking (WTS notification).
    SessionLocked,
    /// The cap poll detected the capture hit the 60 s limit (TD-007).
    CapReached,
    /// The engine produced a transcript (already trimmed).
    TranscriptReady(String),
    /// The engine failed or the watchdog timed out.
    TranscriptFailed(String),
    /// Injection finished. `true` means Pasted; `false` means
    /// PastedRestoreFailed, whose clipboard warn was already logged by
    /// the inject module. Both end the cycle as success.
    InjectFinished(bool),
    /// Injection failed outright.
    InjectFailed(String),
}

/// Side effects the runtime must execute, in the order returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Begin buffering microphone audio.
    StartCapture,
    /// Stop buffering and transcribe the captured audio.
    StopCaptureAndTranscribe,
    /// Stop buffering and drop the audio without transcribing.
    DiscardCapture,
    /// Paste the given text into the focused control.
    Inject(String),
    /// Publish a new overlay state to the UI.
    EmitState(UiState),
}

/// Pure dictation state machine. See the module docs for the
/// decisions behind the ignored (state, input) pairs.
pub struct OrchestratorLogic {
    state: State,
    /// Minimum hold for a release to count as a real dictation
    /// (FR-01.AC-2); shorter presses are discarded. Runtime uses 200.
    min_hold_ms: u64,
}

impl OrchestratorLogic {
    /// New machine in [`State::Idle`] with the given accidental-press
    /// threshold in milliseconds (the runtime passes 200).
    pub fn new(min_hold_ms: u64) -> Self {
        Self {
            state: State::Idle,
            min_hold_ms,
        }
    }

    /// Current state, for the runtime loop and the tests.
    pub fn state(&self) -> State {
        self.state
    }

    /// Feeds one input; returns the commands to execute, in order.
    /// Pairs outside the transition table return no commands and keep
    /// the state unchanged (see the module docs).
    pub fn handle(&mut self, input: Input) -> Vec<Command> {
        match (self.state, input) {
            (State::Idle, Input::ComboPressed) => {
                self.state = State::Recording;
                vec![
                    Command::StartCapture,
                    Command::EmitState(UiState::Listening),
                ]
            }
            (State::Recording, Input::ComboReleased { hold_ms })
                if hold_ms >= self.min_hold_ms =>
            {
                self.state = State::Transcribing;
                vec![
                    Command::StopCaptureAndTranscribe,
                    Command::EmitState(UiState::Transcribing),
                ]
            }
            (State::Recording, Input::ComboReleased { .. }) => {
                // Accidental press (FR-01.AC-2), drop the audio silently.
                self.state = State::Idle;
                vec![
                    Command::DiscardCapture,
                    Command::EmitState(UiState::Hidden),
                ]
            }
            (State::Recording, Input::CapReached) => {
                // Proactive cut at the 60 s cap (TD-007). The physical
                // release arriving later, in Transcribing, is ignored.
                self.state = State::Transcribing;
                vec![
                    Command::StopCaptureAndTranscribe,
                    Command::EmitState(UiState::Transcribing),
                ]
            }
            (State::Recording, Input::SessionLocked) => {
                self.state = State::Idle;
                vec![
                    Command::DiscardCapture,
                    Command::EmitState(UiState::Hidden),
                ]
            }
            (State::Transcribing, Input::TranscriptReady(text)) => {
                if text.is_empty() {
                    // FR-04, an empty transcript means no paste and no
                    // error; the cycle just ends.
                    self.state = State::Idle;
                    vec![Command::EmitState(UiState::Hidden)]
                } else {
                    self.state = State::Injecting;
                    vec![
                        Command::Inject(text),
                        Command::EmitState(UiState::Injecting),
                    ]
                }
            }
            (State::Transcribing, Input::TranscriptFailed(message)) => {
                self.state = State::Idle;
                vec![Command::EmitState(UiState::Error(message))]
            }
            (State::Injecting, Input::InjectFinished(_)) => {
                // PastedRestoreFailed also ends the cycle as success;
                // the inject module already logged the clipboard warn.
                self.state = State::Idle;
                vec![Command::EmitState(UiState::Hidden)]
            }
            (State::Injecting, Input::InjectFailed(message)) => {
                self.state = State::Idle;
                vec![Command::EmitState(UiState::Error(message))]
            }
            // Everything else is deliberately ignored; see the module
            // docs for the reasoning behind each dropped pair.
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Runtime default accidental-press threshold, used by every test.
    const MIN_HOLD: u64 = 200;

    fn machine() -> OrchestratorLogic {
        OrchestratorLogic::new(MIN_HOLD)
    }

    // Drives a fresh machine into Recording.
    fn recording() -> OrchestratorLogic {
        let mut m = machine();
        m.handle(Input::ComboPressed);
        m
    }

    // Drives a fresh machine into Transcribing via a long release.
    fn transcribing() -> OrchestratorLogic {
        let mut m = recording();
        m.handle(Input::ComboReleased { hold_ms: MIN_HOLD });
        m
    }

    // Drives a fresh machine into Injecting.
    fn injecting() -> OrchestratorLogic {
        let mut m = transcribing();
        m.handle(Input::TranscriptReady("hola".into()));
        m
    }

    #[test]
    fn starts_idle() {
        assert_eq!(machine().state(), State::Idle);
    }

    #[test]
    fn idle_combo_pressed_starts_recording() {
        let mut m = machine();
        let cmds = m.handle(Input::ComboPressed);
        assert_eq!(m.state(), State::Recording);
        assert_eq!(
            cmds,
            vec![
                Command::StartCapture,
                Command::EmitState(UiState::Listening)
            ]
        );
    }

    #[test]
    fn recording_release_at_threshold_transcribes() {
        let mut m = recording();
        let cmds = m.handle(Input::ComboReleased { hold_ms: MIN_HOLD });
        assert_eq!(m.state(), State::Transcribing);
        assert_eq!(
            cmds,
            vec![
                Command::StopCaptureAndTranscribe,
                Command::EmitState(UiState::Transcribing)
            ]
        );
    }

    #[test]
    fn recording_long_release_transcribes() {
        let mut m = recording();
        let cmds = m.handle(Input::ComboReleased { hold_ms: 5_000 });
        assert_eq!(m.state(), State::Transcribing);
        assert_eq!(cmds[0], Command::StopCaptureAndTranscribe);
    }

    // FR-01.AC-2, an accidental press is discarded without transcribing.
    #[test]
    fn recording_short_release_discards_capture() {
        let mut m = recording();
        let cmds = m.handle(Input::ComboReleased { hold_ms: MIN_HOLD - 1 });
        assert_eq!(m.state(), State::Idle);
        assert_eq!(
            cmds,
            vec![
                Command::DiscardCapture,
                Command::EmitState(UiState::Hidden)
            ]
        );
    }

    // TD-007, the cap poll cuts the cycle without waiting for release.
    #[test]
    fn recording_cap_reached_cuts_proactively() {
        let mut m = recording();
        let cmds = m.handle(Input::CapReached);
        assert_eq!(m.state(), State::Transcribing);
        assert_eq!(
            cmds,
            vec![
                Command::StopCaptureAndTranscribe,
                Command::EmitState(UiState::Transcribing)
            ]
        );
    }

    // After a cap cut the user eventually releases the combo; that
    // release arrives in Transcribing and must be a no-op.
    #[test]
    fn release_after_cap_cut_is_ignored() {
        let mut m = recording();
        m.handle(Input::CapReached);
        let cmds = m.handle(Input::ComboReleased { hold_ms: 60_000 });
        assert_eq!(m.state(), State::Transcribing);
        assert!(cmds.is_empty());
    }

    #[test]
    fn recording_session_locked_discards() {
        let mut m = recording();
        let cmds = m.handle(Input::SessionLocked);
        assert_eq!(m.state(), State::Idle);
        assert_eq!(
            cmds,
            vec![
                Command::DiscardCapture,
                Command::EmitState(UiState::Hidden)
            ]
        );
    }

    #[test]
    fn transcript_ready_with_text_injects() {
        let mut m = transcribing();
        let cmds = m.handle(Input::TranscriptReady("hola mundo".into()));
        assert_eq!(m.state(), State::Injecting);
        assert_eq!(
            cmds,
            vec![
                Command::Inject("hola mundo".into()),
                Command::EmitState(UiState::Injecting)
            ]
        );
    }

    // FR-04, an empty transcript ends the cycle with no paste and no
    // error surfaced to the user.
    #[test]
    fn transcript_ready_empty_ends_cycle_silently() {
        let mut m = transcribing();
        let cmds = m.handle(Input::TranscriptReady(String::new()));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(cmds, vec![Command::EmitState(UiState::Hidden)]);
    }

    #[test]
    fn transcript_failed_reports_error_and_goes_idle() {
        let mut m = transcribing();
        let cmds = m.handle(Input::TranscriptFailed("GPU_CONTENTION".into()));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(
            cmds,
            vec![Command::EmitState(UiState::Error("GPU_CONTENTION".into()))]
        );
    }

    #[test]
    fn inject_finished_pasted_hides_and_goes_idle() {
        let mut m = injecting();
        let cmds = m.handle(Input::InjectFinished(true));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(cmds, vec![Command::EmitState(UiState::Hidden)]);
    }

    // PastedRestoreFailed still ends the cycle as success; the inject
    // module already logged the clipboard warn.
    #[test]
    fn inject_finished_restore_failed_also_ends_ok() {
        let mut m = injecting();
        let cmds = m.handle(Input::InjectFinished(false));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(cmds, vec![Command::EmitState(UiState::Hidden)]);
    }

    #[test]
    fn inject_failed_reports_error_and_goes_idle() {
        let mut m = injecting();
        let cmds = m.handle(Input::InjectFailed("no focused control".into()));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(
            cmds,
            vec![Command::EmitState(UiState::Error("no focused control".into()))]
        );
    }

    // Robustness, re-pressing the combo mid-cycle never re-enters.
    #[test]
    fn combo_pressed_is_ignored_while_active() {
        for mut m in [recording(), transcribing(), injecting()] {
            let before = m.state();
            let cmds = m.handle(Input::ComboPressed);
            assert!(cmds.is_empty());
            assert_eq!(m.state(), before);
        }
    }

    #[test]
    fn combo_released_in_idle_is_ignored() {
        let mut m = machine();
        let cmds = m.handle(Input::ComboReleased { hold_ms: 1_000 });
        assert_eq!(m.state(), State::Idle);
        assert!(cmds.is_empty());
    }

    // Watchdog case from the module docs, a late engine result after
    // the runtime moved on must never paste.
    #[test]
    fn late_transcript_ready_is_ignored_outside_transcribing() {
        for mut m in [machine(), recording(), injecting()] {
            let before = m.state();
            let cmds = m.handle(Input::TranscriptReady("tarde".into()));
            assert!(cmds.is_empty());
            assert_eq!(m.state(), before);
        }
    }

    #[test]
    fn late_transcript_failed_is_ignored_outside_transcribing() {
        for mut m in [machine(), recording(), injecting()] {
            let before = m.state();
            let cmds = m.handle(Input::TranscriptFailed("tarde".into()));
            assert!(cmds.is_empty());
            assert_eq!(m.state(), before);
        }
    }

    // A stale cap poll result outside Recording is a no-op.
    #[test]
    fn cap_reached_is_ignored_outside_recording() {
        for mut m in [machine(), transcribing(), injecting()] {
            let before = m.state();
            let cmds = m.handle(Input::CapReached);
            assert!(cmds.is_empty());
            assert_eq!(m.state(), before);
        }
    }

    // The audio is pre-lock; the cycle continues (module docs).
    #[test]
    fn session_locked_does_not_abort_transcribing_or_injecting() {
        for mut m in [transcribing(), injecting()] {
            let before = m.state();
            let cmds = m.handle(Input::SessionLocked);
            assert!(cmds.is_empty());
            assert_eq!(m.state(), before);
        }
    }

    #[test]
    fn session_locked_in_idle_is_ignored() {
        let mut m = machine();
        assert!(m.handle(Input::SessionLocked).is_empty());
        assert_eq!(m.state(), State::Idle);
    }

    #[test]
    fn inject_events_are_ignored_outside_injecting() {
        for mut m in [machine(), recording(), transcribing()] {
            let before = m.state();
            assert!(m.handle(Input::InjectFinished(true)).is_empty());
            assert!(m.handle(Input::InjectFailed("x".into())).is_empty());
            assert_eq!(m.state(), before);
        }
    }

    // After any error the next press starts a clean cycle.
    #[test]
    fn cycle_after_error_starts_clean() {
        let mut m = transcribing();
        m.handle(Input::TranscriptFailed("boom".into()));
        assert_eq!(m.state(), State::Idle);
        let cmds = m.handle(Input::ComboPressed);
        assert_eq!(m.state(), State::Recording);
        assert_eq!(cmds[0], Command::StartCapture);
    }

    #[test]
    fn full_happy_cycle_ends_idle() {
        let mut m = machine();
        m.handle(Input::ComboPressed);
        m.handle(Input::ComboReleased { hold_ms: 1_500 });
        m.handle(Input::TranscriptReady("texto final".into()));
        let cmds = m.handle(Input::InjectFinished(true));
        assert_eq!(m.state(), State::Idle);
        assert_eq!(cmds, vec![Command::EmitState(UiState::Hidden)]);
    }
}
