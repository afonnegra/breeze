//! WhisperEngine (FR-04): local transcription with whisper.cpp + CUDA.
//! The model is loaded once and stays resident in VRAM.
//!
//! Design:
//! - `WhisperContext` is created once in `load`; each `transcribe` creates
//!   its own `WhisperState` (no pool — YAGNI until measured).
//! - Concurrency: `Mutex` + `try_lock`; if another transcription is in
//!   flight, `transcribe` returns `Err(EngineError::Busy)`.
//! - NO internal timeout: a timeout over a blocking GPU call cancels
//!   nothing and would leave the lock held (permanent `Busy`). The
//!   watchdog lives in the orchestrator (PHASE 4) over `spawn_blocking`.

use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard, Once, TryLockError};
use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Es,
    En,
}

impl Language {
    pub fn whisper_code(self) -> &'static str {
        match self {
            Language::Es => "es",
            Language::En => "en",
        }
    }
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("transcription already in progress")]
    Busy,
    #[error("transcription failed: {0}")]
    Transcribe(String),
}

// ── Runtime GPU backend detection (TD-003) ──────────────────────────────
//
// whisper.cpp does NOT report CUDA init failure through any return value:
// it logs "ggml_cuda_init: failed to initialize CUDA" / "no GPU found" and
// silently falls back to CPU. That silent fallback is exactly how TD-003
// shipped a CPU-only build undetected. whisper-rs 0.16 exposes no API to
// query the active backend either, so the only reliable runtime signal is
// the log stream: we install our own whisper/ggml log callback (via the
// `raw-api` re-export of whisper-rs-sys), classify the messages emitted
// while the model loads, and record the outcome in process-global state
// that `crate::api::cuda::cuda_available()` reads.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendSignal {
    /// CUDA device found / selected as the active backend.
    GpuInitOk,
    /// CUDA init failed or whisper fell back to CPU.
    GpuInitFailed,
}

/// Classifies a whisper.cpp/ggml log line as evidence of GPU init success
/// or failure. Pure function so the parsing logic is unit-testable.
fn classify_backend_message(text: &str) -> Option<BackendSignal> {
    // Failure lines (captured verbatim during CUDA bring-up):
    //   "ggml_cuda_init: failed to initialize CUDA: (null)"
    //   "whisper_backend_init_gpu: no GPU found"
    if text.contains("failed to initialize CUDA") || text.contains("no GPU found") {
        return Some(BackendSignal::GpuInitFailed);
    }
    // Success lines (whisper.cpp v1.7.x):
    //   "ggml_cuda_init: found 1 CUDA devices:"
    //   "whisper_backend_init_gpu: using device CUDA0 (NVIDIA ...) - N MiB free"
    // "found 0 CUDA devices" is NOT success: a CUDA runtime that
    // initializes but sees no device still means CPU fallback.
    if text.contains("ggml_cuda_init") && text.contains("found") && !text.contains("found 0") {
        return Some(BackendSignal::GpuInitOk);
    }
    if text.contains("using device") && text.contains("CUDA") {
        return Some(BackendSignal::GpuInitOk);
    }
    // Per-load evidence: model weights allocated on a CUDA buffer
    // ("whisper_model_load:  CUDA0 total size = ...") or whisper picked a
    // CUDA backend device ("whisper_backend_init_gpu: device 0: CUDA0").
    // Required because ggml_cuda_init logs only once per process (backend
    // registry init), so 2nd+ model loads in the same process would
    // otherwise produce no in-window success signal at all.
    if (text.contains("whisper_model_load") || text.contains("whisper_backend_init_gpu"))
        && text.contains("CUDA")
    {
        return Some(BackendSignal::GpuInitOk);
    }
    None
}

/// Signals observed during the current `WhisperEngine::load` (reset there).
static LOAD_SAW_GPU_OK: AtomicBool = AtomicBool::new(false);
static LOAD_SAW_GPU_FAIL: AtomicBool = AtomicBool::new(false);

/// Serializes model loads so the two flags above cannot interleave between
/// concurrent loads (only relevant for tests; production loads once).
static LOAD_PROBE_LOCK: Mutex<()> = Mutex::new(());

const CUDA_STATE_UNKNOWN: u8 = 0;
const CUDA_STATE_ACTIVE: u8 = 1;
const CUDA_STATE_FAILED: u8 = 2;

/// Backend actually initialized by the most recent model load, process-wide.
static CUDA_RUNTIME_STATE: AtomicU8 = AtomicU8::new(CUDA_STATE_UNKNOWN);

/// Real CUDA state observed at the last model load: `Some(true)` GPU active,
/// `Some(false)` whisper fell back to CPU, `None` if no engine has loaded
/// yet in this process. Consumed by `crate::api::cuda::cuda_available()`
/// (TD-003: a silent CPU fallback must be detectable at runtime).
pub fn cuda_runtime_active() -> Option<bool> {
    match CUDA_RUNTIME_STATE.load(Ordering::Acquire) {
        CUDA_STATE_ACTIVE => Some(true),
        CUDA_STATE_FAILED => Some(false),
        _ => None,
    }
}

/// Log callback installed into BOTH whisper.cpp and ggml (each emits part of
/// the evidence: ggml owns "ggml_cuda_init: ...", whisper owns
/// "whisper_backend_init_gpu: ..."). Replaces the default stderr output:
/// messages are forwarded to `tracing` and scanned for backend signals.
///
/// Must never unwind into the C caller: since Rust 1.81 unwinding out of an
/// `extern "C"` function is a guaranteed process abort (not UB), so the body
/// is wrapped in `catch_unwind` — a panicking tracing subscriber must not
/// abort the process over a ggml log line.
unsafe extern "C" fn backend_log_trampoline(
    level: whisper_rs::whisper_rs_sys::ggml_log_level,
    text: *const c_char,
    _user_data: *mut c_void,
) {
    if text.is_null() {
        return;
    }
    // The panic (if any) is swallowed: there is nowhere safe to report it
    // from inside a C log callback.
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: whisper.cpp/ggml pass a valid NUL-terminated C string;
        // lossy conversion tolerates any non-UTF-8 bytes.
        let msg = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        on_backend_log(level, &msg);
    });
}

/// Safe part of the log callback: updates the backend-signal flags and
/// forwards the message to `tracing` at an equivalent level.
fn on_backend_log(level: whisper_rs::whisper_rs_sys::ggml_log_level, msg: &str) {
    match classify_backend_message(msg) {
        Some(BackendSignal::GpuInitOk) => LOAD_SAW_GPU_OK.store(true, Ordering::Release),
        Some(BackendSignal::GpuInitFailed) => LOAD_SAW_GPU_FAIL.store(true, Ordering::Release),
        None => {}
    }
    let trimmed = msg.trim();
    if trimmed.is_empty() {
        return;
    }
    use whisper_rs::whisper_rs_sys as sys;
    match level {
        sys::ggml_log_level_GGML_LOG_LEVEL_ERROR => {
            tracing::error!(target: "whisper_cpp", "{trimmed}");
        }
        sys::ggml_log_level_GGML_LOG_LEVEL_WARN => {
            tracing::warn!(target: "whisper_cpp", "{trimmed}");
        }
        sys::ggml_log_level_GGML_LOG_LEVEL_INFO => {
            tracing::info!(target: "whisper_cpp", "{trimmed}");
        }
        // DEBUG / NONE / CONT (continuation of a previous line) / unknown.
        _ => tracing::debug!(target: "whisper_cpp", "{trimmed}"),
    }
}

static LOG_HOOK_INSTALL: Once = Once::new();

/// Installs `backend_log_trampoline` as the global whisper.cpp/ggml log
/// callback. Idempotent. NOTE: do not also call
/// `whisper_rs::install_logging_hooks()` — the last `whisper_log_set` wins
/// and would bypass the backend probe.
fn install_backend_log_hook() {
    LOG_HOOK_INSTALL.call_once(|| {
        // SAFETY: the trampoline is a 'static fn pointer with the exact
        // ggml_log_callback ABI; user_data is unused (null).
        unsafe {
            whisper_rs::whisper_rs_sys::whisper_log_set(
                Some(backend_log_trampoline),
                std::ptr::null_mut(),
            );
            whisper_rs::whisper_rs_sys::ggml_log_set(
                Some(backend_log_trampoline),
                std::ptr::null_mut(),
            );
        }
    });
}

/// Acquires the single-transcription guard without blocking.
///
/// - `WouldBlock` → another transcription is in flight → `Err(Busy)`.
/// - `Poisoned` → a previous transcription panicked while holding the
///   guard. The `WhisperContext` is safe to reuse (the panicking call's
///   `WhisperState` was dropped during unwinding), so the guard is
///   recovered instead of reporting `Busy` forever. Same pattern as
///   `INIT_LOCK` in `api::transcription` and `LOAD_PROBE_LOCK` above.
fn acquire_transcription_guard(lock: &Mutex<()>) -> Result<MutexGuard<'_, ()>, EngineError> {
    match lock.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => Err(EngineError::Busy),
        Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
    }
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    /// Guarantees at most 1 transcription in flight (NFR: the GPU is not shared).
    lock: Mutex<()>,
    /// Whether CUDA actually initialized when this engine loaded (TD-003).
    gpu_active: bool,
}

impl WhisperEngine {
    /// Loads the model once; it stays resident in VRAM (GPU by default
    /// with the `cuda` feature — `use_gpu` is already `true` in 0.16).
    pub fn load(model_path: &Path) -> Result<Self, EngineError> {
        install_backend_log_hook();

        // Serialize loads: the SAW flags are process-global (TD-003 probe).
        // A poisoned lock only means another load panicked; the flags are
        // reset right below, so recovering the guard is safe.
        let _probe_guard = LOAD_PROBE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        LOAD_SAW_GPU_OK.store(false, Ordering::Release);
        LOAD_SAW_GPU_FAIL.store(false, Ordering::Release);

        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| EngineError::Load(e.to_string()))?;

        // GPU is active only with positive evidence and no failure signal:
        // a silent CPU fallback (TD-003) must never be reported as GPU.
        let gpu_active = LOAD_SAW_GPU_OK.load(Ordering::Acquire)
            && !LOAD_SAW_GPU_FAIL.load(Ordering::Acquire);
        CUDA_RUNTIME_STATE.store(
            if gpu_active {
                CUDA_STATE_ACTIVE
            } else {
                CUDA_STATE_FAILED
            },
            Ordering::Release,
        );

        if gpu_active {
            tracing::info!(model_path = %model_path.display(), "whisper backend: GPU");
        } else {
            tracing::warn!(
                model_path = %model_path.display(),
                "whisper backend: CPU (WARNING: NFR-01/NFR-14 violated) - CUDA did not initialize, see docs/BUILDING.md (CUDA toolkit / driver compatibility)"
            );
        }

        Ok(Self {
            ctx,
            lock: Mutex::new(()),
            gpu_active,
        })
    }

    /// True if CUDA initialized for this engine (the model runs on GPU).
    pub fn gpu_active(&self) -> bool {
        self.gpu_active
    }

    /// PCM 16-bit 16 kHz mono → trimmed text.
    /// Returns `Err(Busy)` if another transcription is already in flight.
    pub fn transcribe(&self, pcm: &[i16], lang: Language) -> Result<String, EngineError> {
        // An empty capture is simply an empty transcription: nothing to
        // send to the GPU (whisper_full fails on empty input).
        if pcm.is_empty() {
            return Ok(String::new());
        }

        let _guard = acquire_transcription_guard(&self.lock)?;

        let samples = pcm_i16_to_f32(pcm);
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| EngineError::Transcribe(format!("create_state: {e}")))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(lang.whisper_code()));
        params.set_translate(false);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // TD-009 defense-in-depth against silence hallucinations. The
        // vendored whisper.cpp (v1.7.x) DOES use no_speech_thold in its
        // decode loop (whisper.cpp:7585, is_no_speech = no_speech_prob >
        // thold && avg_logprob < logprob_thold suppresses the segment);
        // the whisper-rs 0.16 doc-comment saying "not implemented (as of
        // v1.3.0)" is stale. 0.6 matches the whisper.cpp default - set
        // explicitly so the value is pinned and documented. The primary
        // defense is the RMS silence gate in orchestrator::runtime.
        params.set_no_speech_thold(0.6);

        state
            .full(params, &samples)
            .map_err(|e| EngineError::Transcribe(format!("full: {e}")))?;

        let n_segments = state.full_n_segments();
        let mut text = String::new();
        for i in 0..n_segments {
            let segment = state
                .get_segment(i)
                .ok_or_else(|| EngineError::Transcribe(format!("segment {i} out of bounds")))?;
            let segment_text = segment
                .to_str_lossy()
                .map_err(|e| EngineError::Transcribe(format!("segment {i} text: {e}")))?;
            text.push_str(&segment_text);
        }
        Ok(text.trim().to_owned())
    }
}

/// Converts i16 PCM to f32 normalized to [-1.0, 1.0].
pub fn pcm_i16_to_f32(pcm: &[i16]) -> Vec<f32> {
    pcm.iter().map(|&x| f32::from(x) / 32768.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── Unit tests (no GPU) ──────────────────────────────────────────────

    #[test]
    fn language_codes_are_correct() {
        assert_eq!(Language::Es.whisper_code(), "es");
        assert_eq!(Language::En.whisper_code(), "en");
    }

    #[test]
    fn pcm_conversion_normalizes() {
        let out = pcm_i16_to_f32(&[0, i16::MAX, i16::MIN]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 1.0).abs() < 1e-4, "i16::MAX should map to ~1.0, got {}", out[1]);
        assert_eq!(out[2], -1.0);
    }

    #[test]
    fn pcm_conversion_empty() {
        assert!(pcm_i16_to_f32(&[]).is_empty());
    }

    // ── Transcription guard (review B-1, unit, no GPU) ──────────────────

    #[test]
    fn transcription_guard_busy_while_held() {
        let lock = Mutex::new(());
        let held = lock.lock().expect("fresh lock cannot be poisoned");
        assert!(matches!(
            acquire_transcription_guard(&lock),
            Err(EngineError::Busy)
        ));
        drop(held);
        assert!(acquire_transcription_guard(&lock).is_ok());
    }

    #[test]
    fn transcription_guard_recovers_from_poison() {
        let lock = Mutex::new(());
        // Poison the lock: panic while the guard is held.
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = lock.lock().expect("fresh lock cannot be poisoned");
            panic!("poisoning the transcription lock on purpose");
        }));
        assert!(panic_result.is_err(), "closure must panic to poison the lock");
        assert!(lock.is_poisoned());
        // A poisoned (but free) lock must be recovered — not Busy forever.
        assert!(
            acquire_transcription_guard(&lock).is_ok(),
            "poisoned lock must be recovered, not treated as Busy"
        );
    }

    // ── Backend signal classification (TD-003, unit, no GPU) ────────────
    // Literal lines below are real whisper.cpp/ggml output: the failure
    // ones were captured during CUDA bring-up; the success ones are
    // whisper.cpp v1.7.x GPU init output.

    #[test]
    fn classifies_cuda_init_failure_as_failed() {
        assert_eq!(
            classify_backend_message("ggml_cuda_init: failed to initialize CUDA: (null)"),
            Some(BackendSignal::GpuInitFailed)
        );
    }

    #[test]
    fn classifies_no_gpu_found_as_failed() {
        assert_eq!(
            classify_backend_message("whisper_backend_init_gpu: no GPU found"),
            Some(BackendSignal::GpuInitFailed)
        );
    }

    #[test]
    fn classifies_cuda_devices_found_as_ok() {
        assert_eq!(
            classify_backend_message("ggml_cuda_init: found 1 CUDA devices:"),
            Some(BackendSignal::GpuInitOk)
        );
    }

    #[test]
    fn does_not_classify_zero_cuda_devices_as_ok() {
        assert_eq!(
            classify_backend_message("ggml_cuda_init: found 0 CUDA devices"),
            None
        );
    }

    #[test]
    fn classifies_using_cuda_device_as_ok() {
        assert_eq!(
            classify_backend_message(
                "whisper_backend_init_gpu: using device CUDA0 (NVIDIA GeForce RTX 4070 Laptop GPU) - 7013 MiB free"
            ),
            Some(BackendSignal::GpuInitOk)
        );
    }

    #[test]
    fn classifies_model_load_on_cuda_as_ok() {
        assert_eq!(
            classify_backend_message("whisper_model_load:        CUDA0 total size =   573.45 MB"),
            Some(BackendSignal::GpuInitOk)
        );
    }

    #[test]
    fn classifies_backend_init_gpu_cuda_device_as_ok() {
        assert_eq!(
            classify_backend_message("whisper_backend_init_gpu: device 0: CUDA0 (type: 1)"),
            Some(BackendSignal::GpuInitOk)
        );
    }

    #[test]
    fn ignores_model_load_without_cuda() {
        assert_eq!(
            classify_backend_message("whisper_model_load: model size    =  573.40 MB"),
            None
        );
    }

    #[test]
    fn ignores_unrelated_messages() {
        assert_eq!(
            classify_backend_message(
                "whisper_init_from_file_with_params_no_state: loading model from 'ggml-large-v3-turbo.bin'"
            ),
            None
        );
        assert_eq!(
            classify_backend_message(
                "  Device 0: NVIDIA GeForce RTX 4070 Laptop GPU, compute capability 8.9, VMM: yes"
            ),
            None
        );
        assert_eq!(classify_backend_message(""), None);
    }

    // ── Integration tests (GPU + real model + fixtures; `--ignored`) ────

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("test-fixtures")
            .join(name)
    }

    /// Reads a WAV fixture (must be 16 kHz mono 16-bit; see generate-fixtures.ps1).
    fn load_fixture_pcm(name: &str) -> Vec<i16> {
        let path = fixture_path(name);
        let mut reader = hound::WavReader::open(&path).unwrap_or_else(|e| {
            panic!(
                "fixture {} missing ({e}); run: powershell.exe -NoProfile -File test-fixtures\\generate-fixtures.ps1",
                path.display()
            )
        });
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000, "fixture must be 16 kHz");
        assert_eq!(spec.channels, 1, "fixture must be mono");
        assert_eq!(spec.bits_per_sample, 16, "fixture must be 16-bit");
        reader
            .samples::<i16>()
            .map(|s| s.expect("valid PCM sample"))
            .collect()
    }

    /// whisper.cpp logs flow through our tracing hook now (not stderr), so
    /// integration runs need a subscriber or the GPU init evidence would be
    /// invisible in the test output.
    fn init_test_tracing() {
        static TRACING: Once = Once::new();
        TRACING.call_once(|| {
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::stderr)
                .try_init();
        });
    }

    fn load_real_engine() -> WhisperEngine {
        init_test_tracing();
        let dirs = crate::model::default_search_dirs();
        let path = crate::model::locate_model(&dirs).expect("real model must exist for this test");
        let engine = WhisperEngine::load(&path).expect("engine must load");
        // Every GPU integration test must actually run on GPU: a CPU
        // fallback is the exact silent failure TD-003 exists to catch.
        assert!(
            engine.gpu_active(),
            "whisper must run on GPU on every load; CPU fallback = driver/toolkit mismatch (TD-003)"
        );
        engine
    }

    #[test]
    #[ignore = "requiere GPU + modelo real en %LOCALAPPDATA%; carga ~1-3 s"]
    fn engine_loads_real_model() {
        let engine = load_real_engine();
        assert!(
            engine.gpu_active(),
            "whisper must run on GPU; a CPU fallback means driver/toolkit mismatch (TD-003)"
        );
        assert_eq!(cuda_runtime_active(), Some(true));
        // Review S-3 guard: empty PCM must short-circuit to Ok("") without
        // touching the GPU.
        assert_eq!(
            engine
                .transcribe(&[], Language::En)
                .expect("empty PCM must be Ok"),
            ""
        );
    }

    #[test]
    #[ignore = "requiere GPU + modelo + fixtures; fixture ES generado con voz TTS en-US (aproximación marcada)"]
    fn transcribes_spanish_fixture() {
        let engine = load_real_engine();
        let pcm = load_fixture_pcm("es-corta.wav");
        let t0 = std::time::Instant::now();
        let text = engine
            .transcribe(&pcm, Language::Es)
            .expect("transcription must succeed");
        eprintln!("[itest] es-corta.wav transcribed in {:?}: {text:?}", t0.elapsed());
        let lower = text.to_lowercase();
        assert!(lower.contains("prueba"), "expected 'prueba' in: {text:?}");
        assert!(lower.contains("transcripci"), "expected 'transcripci…' in: {text:?}");
    }

    #[test]
    #[ignore = "requiere GPU + modelo + fixtures"]
    fn transcribes_english_fixture() {
        let engine = load_real_engine();
        let pcm = load_fixture_pcm("en-corta.wav");
        let t0 = std::time::Instant::now();
        let text = engine
            .transcribe(&pcm, Language::En)
            .expect("transcription must succeed");
        eprintln!("[itest] en-corta.wav transcribed in {:?}: {text:?}", t0.elapsed());
        let lower = text.to_lowercase();
        assert!(lower.contains("transcription"), "expected 'transcription' in: {text:?}");
        assert!(lower.contains("test"), "expected 'test' in: {text:?}");
    }

    /// Latency benchmark (NFR-01 partial, PHASE 1 Task 6): p50/p95 of pure
    /// transcription per clip duration, 10 runs per fixture.
    /// Phase threshold: 5 s clip → p95 ≤ 400 ms (the total NFR-01 budget
    /// is 500 ms and includes the paste).
    /// Run with:
    ///   cargo test --lib bench_latency -- --ignored --nocapture --test-threads=1
    #[test]
    #[ignore = "benchmark; requiere GPU + modelo + fixtures en-5s/15s/30s"]
    fn bench_latency() {
        const RUNS: usize = 10;

        /// Nearest-rank percentile over the ascending sorted sample.
        fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
            let rank = ((sorted_ms.len() as f64) * p / 100.0).ceil() as usize;
            sorted_ms[rank.max(1) - 1]
        }

        let engine = load_real_engine();

        // Warmup: the first transcription after `load` pays the CUDA warmup
        // (kernels + buffers); it runs once and is NOT measured.
        let warmup_pcm = load_fixture_pcm("en-5s.wav");
        let t0 = std::time::Instant::now();
        engine
            .transcribe(&warmup_pcm, Language::En)
            .expect("warmup transcription must succeed");
        println!("warmup (en-5s.wav, no medido): {:?}", t0.elapsed());
        println!();
        println!("| fixture    | audio (s) | p50 (ms) | p95 (ms) | min (ms) | max (ms) |");
        println!("|------------|-----------|----------|----------|----------|----------|");

        for name in ["en-5s.wav", "en-15s.wav", "en-30s.wav"] {
            let pcm = load_fixture_pcm(name);
            let audio_secs = pcm.len() as f64 / 16_000.0;
            let mut samples_ms: Vec<f64> = Vec::with_capacity(RUNS);
            for _ in 0..RUNS {
                let t0 = std::time::Instant::now();
                engine
                    .transcribe(&pcm, Language::En)
                    .expect("bench transcription must succeed");
                samples_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
            }
            samples_ms.sort_by(|a, b| a.total_cmp(b));
            println!(
                "| {name:<10} | {audio_secs:>9.1} | {:>8.1} | {:>8.1} | {:>8.1} | {:>8.1} |",
                percentile(&samples_ms, 50.0),
                percentile(&samples_ms, 95.0),
                samples_ms[0],
                samples_ms[RUNS - 1],
            );
        }
    }

    /// Flakiness-tolerant by design: uses the ~30 s fixture (the first
    /// transcription after `load` also pays the CUDA warmup, several
    /// seconds) + a sleep after the spawn so the race window is measured
    /// in seconds, not milliseconds.
    #[test]
    #[ignore = "requiere GPU + modelo + fixture en-30s.wav; ~30 s de audio"]
    fn rejects_concurrent_transcription() {
        let engine = load_real_engine();
        let pcm_long = load_fixture_pcm("en-30s.wav");
        let pcm_short = load_fixture_pcm("en-corta.wav");
        std::thread::scope(|s| {
            s.spawn(|| {
                engine
                    .transcribe(&pcm_long, Language::En)
                    .expect("long transcription must succeed");
            });
            std::thread::sleep(std::time::Duration::from_millis(500));
            let result = engine.transcribe(&pcm_short, Language::En);
            assert!(
                matches!(result, Err(EngineError::Busy)),
                "expected Err(Busy) while long transcription in flight, got {result:?}"
            );
        });
    }
}
