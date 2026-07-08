//! FRB transcription API (FR-04 + FR-09): init_engine + transcribe_pcm.
//!
//! The engine lives in a Rust-side global `OnceLock<WhisperEngine>` — Dart
//! does not manage the whisper context lifetime. Minimal API for PHASE 1;
//! the orchestrator (PHASE 4) will consume it.

use std::sync::{Mutex, OnceLock};

use crate::model::{default_search_dirs, locate_model, verify_sha256, ModelError, MODEL_SHA256};
use crate::whisper_engine::{EngineError, Language as EngineLanguage, WhisperEngine};

static ENGINE: OnceLock<WhisperEngine> = OnceLock::new();

/// Path of the model ENGINE was initialized with (set before ENGINE, under
/// INIT_LOCK, so re-entrant calls can return it).
static MODEL_PATH: OnceLock<String> = OnceLock::new();

/// Serializes `init_engine`: FRB dispatches async calls on a thread pool,
/// and without this two concurrent inits would load the model twice
/// (~1 GB of transient VRAM) only to discard one copy.
static INIT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionLanguage {
    Es,
    En,
}

impl From<TranscriptionLanguage> for EngineLanguage {
    fn from(lang: TranscriptionLanguage) -> Self {
        match lang {
            TranscriptionLanguage::Es => EngineLanguage::Es,
            TranscriptionLanguage::En => EngineLanguage::En,
        }
    }
}

/// Typed error exposed to Dart (FRB 2.12 translates Rust enums → Dart).
#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("model not found in any search path")]
    ModelMissing,
    #[error("model failed integrity check (size or hash mismatch)")]
    ModelCorrupt,
    #[error("engine failed to load: {0}")]
    EngineLoadFailed(String),
    #[error("transcription already in progress")]
    Busy,
    #[error("transcription failed: {0}")]
    TranscribeFailed(String),
    #[error("engine not initialized: call init_engine first")]
    NotInitialized,
}

impl From<ModelError> for TranscriptionError {
    fn from(err: ModelError) -> Self {
        match err {
            ModelError::NotFound => TranscriptionError::ModelMissing,
            ModelError::SizeMismatch { .. } | ModelError::HashMismatch => {
                TranscriptionError::ModelCorrupt
            }
            ModelError::Io(e) => TranscriptionError::EngineLoadFailed(e.to_string()),
        }
    }
}

impl From<EngineError> for TranscriptionError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::Load(msg) => TranscriptionError::EngineLoadFailed(msg),
            EngineError::Busy => TranscriptionError::Busy,
            EngineError::Transcribe(msg) => TranscriptionError::TranscribeFailed(msg),
        }
    }
}

/// Locates the model, optionally verifies the full hash, and loads the
/// engine. Returns the path of the model in use.
///
/// `full_verify`: true only on first launch (FR-09). Idempotent: if the
/// engine is already loaded it returns the original path without reloading
/// (FR-04.AC-4: the model is loaded exactly once). CAVEAT: in that case a
/// requested `full_verify` is NOT performed (a warning is logged); the
/// on-demand verification path needs a separate `verify_model()` API —
/// implemented by [`verify_model`].
pub fn init_engine(full_verify: bool) -> Result<String, TranscriptionError> {
    // A poisoned lock only means another init panicked; the protected
    // state (OnceLock) is still consistent — recover the guard.
    let _guard = INIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if ENGINE.get().is_some() {
        if let Some(path) = MODEL_PATH.get() {
            if full_verify {
                tracing::warn!(
                    model_path = %path,
                    "init_engine: full_verify requested but engine already loaded; verification skipped (TD-004)"
                );
            }
            tracing::info!(model_path = %path, "init_engine: already initialized, reusing engine");
            return Ok(path.clone());
        }
    }

    let dirs = default_search_dirs();
    let model_path = locate_model(&dirs)
        .inspect_err(|e| tracing::warn!(error = %e, "init_engine: locate_model failed"))?;

    if full_verify {
        verify_sha256(&model_path, MODEL_SHA256)
            .inspect_err(|e| tracing::warn!(error = %e, "init_engine: sha256 verification failed"))?;
    }

    let engine = WhisperEngine::load(&model_path)
        .inspect_err(|e| tracing::warn!(error = %e, "init_engine: engine load failed"))?;
    let gpu_active = engine.gpu_active();

    let path_str = model_path.display().to_string();
    // Under INIT_LOCK and with ENGINE empty the `set` calls cannot fail;
    // the Err is ignored defensively (never unwrap in production).
    let _ = MODEL_PATH.set(path_str.clone());
    let _ = ENGINE.set(engine);

    tracing::info!(
        model_path = %path_str,
        full_verify,
        gpu_active,
        "init_engine: whisper engine loaded and resident"
    );
    Ok(path_str)
}

/// On-demand full integrity check (FR-09.AC-2, closes TD-004). Locates
/// the model and verifies its SHA-256 ALWAYS, independent of whether the
/// engine is loaded, and WITHOUT reloading it. Returns the verified
/// model path on success. The tray "Verify model" item calls this; a
/// failure maps to ModelMissing/ModelCorrupt exactly like startup, so
/// the app can surface the reinstall guidance.
pub fn verify_model() -> Result<String, TranscriptionError> {
    verify_model_in(&default_search_dirs())
}

/// Testable core of [`verify_model`]: the search dirs are injected so
/// the pure locate+verify path can be exercised against a temp model
/// without the real file or the global engine. Never touches ENGINE.
fn verify_model_in(dirs: &[std::path::PathBuf]) -> Result<String, TranscriptionError> {
    let model_path = locate_model(dirs)
        .inspect_err(|e| tracing::warn!(error = %e, "verify_model: locate_model failed"))?;
    verify_sha256(&model_path, MODEL_SHA256)
        .inspect_err(|e| tracing::warn!(error = %e, "verify_model: sha256 verification failed"))?;
    let path_str = model_path.display().to_string();
    tracing::info!(model_path = %path_str, "verify_model: integrity check passed");
    Ok(path_str)
}

/// Transcribes PCM 16-bit 16 kHz mono. Returns trimmed text.
pub fn transcribe_pcm(
    pcm: Vec<i16>,
    lang: TranscriptionLanguage,
) -> Result<String, TranscriptionError> {
    let engine = ENGINE.get().ok_or(TranscriptionError::NotInitialized)?;
    let text = engine
        .transcribe(&pcm, lang.into())
        .inspect_err(|e| tracing::warn!(error = %e, "transcribe_pcm failed"))?;
    Ok(text)
}

/// True if the engine was already initialized in this process.
pub fn engine_is_ready() -> bool {
    ENGINE.get().is_some()
}

/// Global transcription language (FASE 4, design decision 6). Defaults
/// to Spanish; the UI selector arrives in FASE 5 (FR-07). A Mutex keeps
/// updates trivially safe across the orchestrator thread and FRB calls.
static LANGUAGE: Mutex<TranscriptionLanguage> = Mutex::new(TranscriptionLanguage::Es);

/// Sets the global language used for subsequent transcriptions.
pub fn set_language(lang: TranscriptionLanguage) {
    let mut guard = LANGUAGE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = lang;
    tracing::info!(language = ?lang, "set_language");
}

/// Current global transcription language (default Spanish).
pub fn get_language() -> TranscriptionLanguage {
    *LANGUAGE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs in a test process where nobody called init_engine: the
    /// OnceLock must be empty. (Tests that DO initialize the real engine
    /// are marked #[ignore] in whisper_engine.)
    #[test]
    fn engine_is_ready_false_before_init() {
        assert!(!engine_is_ready());
    }

    /// Default, set and get exercised in one test because the global
    /// is shared across parallel test threads; splitting the roundtrip
    /// into separate tests would race.
    #[test]
    fn language_defaults_to_es_and_roundtrips() {
        assert_eq!(get_language(), TranscriptionLanguage::Es);
        set_language(TranscriptionLanguage::En);
        assert_eq!(get_language(), TranscriptionLanguage::En);
        set_language(TranscriptionLanguage::Es);
        assert_eq!(get_language(), TranscriptionLanguage::Es);
    }

    #[test]
    fn transcribe_pcm_before_init_is_not_initialized() {
        let result = transcribe_pcm(vec![0i16; 16_000], TranscriptionLanguage::Es);
        assert!(matches!(result, Err(TranscriptionError::NotInitialized)));
    }

    #[test]
    fn model_error_maps_to_typed_variants() {
        assert!(matches!(
            TranscriptionError::from(ModelError::NotFound),
            TranscriptionError::ModelMissing
        ));
        assert!(matches!(
            TranscriptionError::from(ModelError::HashMismatch),
            TranscriptionError::ModelCorrupt
        ));
        assert!(matches!(
            TranscriptionError::from(ModelError::SizeMismatch {
                expected: 1,
                actual: 2
            }),
            TranscriptionError::ModelCorrupt
        ));
    }

    #[test]
    fn engine_error_maps_to_typed_variants() {
        assert!(matches!(
            TranscriptionError::from(EngineError::Busy),
            TranscriptionError::Busy
        ));
        assert!(matches!(
            TranscriptionError::from(EngineError::Load("x".into())),
            TranscriptionError::EngineLoadFailed(_)
        ));
        assert!(matches!(
            TranscriptionError::from(EngineError::Transcribe("x".into())),
            TranscriptionError::TranscribeFailed(_)
        ));
    }

    // TD-004 / FR-09.AC-2: verify_model locates + hashes ALWAYS, with no
    // engine involvement. The inner `verify_model_in` takes explicit dirs
    // so it is testable without touching the global engine or the real
    // model. A file with the right size but wrong content must map to
    // ModelCorrupt (hash mismatch), not to a success or a panic.
    #[test]
    fn verify_model_in_reports_corrupt_on_wrong_content() {
        use std::fs::File;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::model::MODEL_FILE_NAME);
        let file = File::create(&path).unwrap();
        file.set_len(crate::model::MODEL_SIZE_BYTES).unwrap();
        drop(file);
        let dirs = vec![dir.path().to_path_buf()];
        let result = verify_model_in(&dirs);
        assert!(matches!(result, Err(TranscriptionError::ModelCorrupt)));
    }

    #[test]
    fn verify_model_in_reports_missing_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        let result = verify_model_in(&dirs);
        assert!(matches!(result, Err(TranscriptionError::ModelMissing)));
    }

    #[test]
    fn verify_model_in_does_not_require_engine() {
        assert!(!engine_is_ready());
        let dir = tempfile::tempdir().unwrap();
        let dirs = vec![dir.path().to_path_buf()];
        assert!(verify_model_in(&dirs).is_err());
        assert!(!engine_is_ready());
    }
}
