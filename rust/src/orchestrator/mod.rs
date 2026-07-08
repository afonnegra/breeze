//! Dictation orchestrator (FASE 4).
//!
//! Split in two layers. [`logic`] is the pure state machine that maps
//! (state, input) pairs to runtime commands with zero side effects, so
//! the full transition table is unit-testable. [`runtime`] owns the
//! dedicated thread that feeds hotkey, cap-polling, transcription and
//! injection events into the logic and executes the returned commands
//! against the real modules (or fakes, in tests).

pub mod logic;
pub mod runtime;
