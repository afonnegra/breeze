//! Microphone capture (FR-02).
//!
//! `AudioCapture` owns a pre-warmed cpal (WASAPI shared-mode) input
//! stream in the device's native format. The stream callback always
//! runs; samples are accumulated only while recording is active.
//! On stop, the buffered audio is converted (downmix to mono +
//! resample to 16 kHz + f32 -> i16) into PCM ready for
//! `WhisperEngine::transcribe`.
//!
//! # Design notes
//!
//! - **Native format, never 16 kHz at the device.** WASAPI shared mode
//!   serves the mix format (typically 44.1/48 kHz float stereo);
//!   requesting 16 kHz would fail or force exclusive mode. We capture
//!   whatever `default_input_config()` reports and convert at stop
//!   time via [`convert`].
//! - **The stream is always playing.** `prewarm()` builds and starts
//!   the stream once; `start_buffer()`/`stop_buffer()` only toggle an
//!   `AtomicBool`, so the hotkey -> first-sample path (NFR-02) never
//!   waits for WASAPI initialization.
//! - **60 s cap (FR-02).** The callback stops accumulating once the
//!   buffer holds the native-format equivalent of the configured max
//!   duration and marks the capture as truncated. The cap is
//!   injectable via [`AudioCapture::prewarm_with_max_duration`] for
//!   tests.
//! - **NFR-11 (device change) is deferred to FASE 6** per the master
//!   plan; a device disconnect surfaces as stream error-callback warns.

pub mod convert;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use tracing::{debug, info, warn};

use convert::{downmix_to_mono, f32_to_i16, resample_to_16k, TARGET_SAMPLE_RATE};

/// Default recording cap (FR-02): 60 seconds.
const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(60);

/// Errors creating or starting the capture stream.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// No default input device is available on this system.
    #[error("no default audio input device available")]
    NoDevice,
    /// Querying the device config or building the stream failed.
    #[error("failed to build input stream: {0}")]
    StreamBuild(String),
    /// The stream was built but could not be started.
    #[error("failed to start input stream: {0}")]
    StreamPlay(String),
}

/// What the device watcher should do this tick (NFR-11). Pure so the
/// policy is unit-testable without devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    /// Stream healthy and still on the default device.
    Keep,
    /// Default changed or stream died while idle: rebuild now.
    Rebuild,
    /// A capture is in flight (never yank a dictation) or there is no
    /// device to rebuild onto yet: retry next tick.
    Defer,
}

/// Watcher policy: compare the system default input device against the
/// one the pre-warmed stream was built on, factoring in whether a
/// capture is in flight and whether the stream already errored out.
pub fn device_watch_action(
    current_default: Option<&str>,
    active_device: &str,
    recording: bool,
    failed: bool,
) -> WatchAction {
    let changed = matches!(current_default, Some(name) if name != active_device);
    if !changed && !failed {
        return WatchAction::Keep;
    }
    if recording || current_default.is_none() {
        return WatchAction::Defer;
    }
    WatchAction::Rebuild
}

/// A finished capture, converted to the format Whisper expects.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    /// 16 kHz mono 16-bit PCM.
    pub pcm: Vec<i16>,
    /// Duration of the captured audio, derived from the native frame
    /// count (i.e. how much audio is actually in `pcm`, not wall time).
    pub duration: Duration,
    /// True if the capture hit the configured cap and audio was lost.
    pub truncated: bool,
}

/// Buffer half of the state shared with the stream callback.
struct BufferState {
    /// Interleaved samples in the device's native rate/channel layout.
    samples: Vec<f32>,
    /// When the first callback delivered samples after `start_buffer`.
    /// Consumed by the NFR-02 latency harness.
    first_sample_at: Option<Instant>,
    /// When `start_buffer` was called (wall clock, for logging).
    started_at: Option<Instant>,
    /// The cap was hit and samples were dropped.
    truncated: bool,
}

/// State shared between [`AudioCapture`] and the cpal stream callback.
struct Shared {
    /// Accumulate samples only while true. Toggled by
    /// `start_buffer`/`stop_buffer`; read by every callback.
    recording: AtomicBool,
    buffer: Mutex<BufferState>,
    /// Cap in native samples (frames x channels).
    max_samples: usize,
    /// Set by the stream error-callback when the backend fails or the
    /// device is unplugged (NFR-11). The watcher reads it to rebuild.
    stream_failed: AtomicBool,
}

/// Lock that shrugs off poisoning: a panicking test thread must not
/// wedge the audio callback (the buffer content stays meaningful).
fn lock_buffer(shared: &Shared) -> std::sync::MutexGuard<'_, BufferState> {
    shared
        .buffer
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Append converted samples to the shared buffer, honoring the
/// recording flag and the cap. Runs on the cpal callback thread.
fn push_samples(shared: &Shared, samples: impl ExactSizeIterator<Item = f32>) {
    if !shared.recording.load(Ordering::Acquire) {
        return;
    }
    let incoming = samples.len();
    let mut buf = lock_buffer(shared);
    if buf.first_sample_at.is_none() && incoming > 0 {
        buf.first_sample_at = Some(Instant::now());
    }
    let room = shared.max_samples.saturating_sub(buf.samples.len());
    if incoming > room {
        buf.truncated = true;
    }
    if room > 0 {
        buf.samples.extend(samples.take(room));
    }
}

/// True if the current capture hit the cap and dropped samples.
/// Extracted from [`AudioCapture::is_truncated`] so the cap logic is
/// unit-testable without a real input device.
fn buffer_truncated(shared: &Shared) -> bool {
    lock_buffer(shared).truncated
}

/// Pre-warmed microphone capture (FR-02). See module docs.
pub struct AudioCapture {
    /// Held only to keep the stream alive; dropped stream = capture over.
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    /// Native sample rate reported by the device.
    sample_rate: u32,
    /// Native channel count reported by the device.
    channels: u16,
    /// Name of the input device this stream was built on (NFR-11).
    device_name: String,
}

impl AudioCapture {
    /// Open and start the default input device with the default 60 s
    /// cap (FR-02). The stream runs (and discards samples) until
    /// [`Self::start_buffer`] flips the recording flag.
    pub fn prewarm() -> Result<Self, CaptureError> {
        Self::prewarm_with_max_duration(DEFAULT_MAX_DURATION)
    }

    /// [`Self::prewarm`] with an injectable cap, used by tests to
    /// exercise truncation without recording 60 real seconds.
    pub fn prewarm_with_max_duration(max_duration: Duration) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(CaptureError::NoDevice)?;
        let device_name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| "<unknown>".into());
        let supported = device
            .default_input_config()
            .map_err(|e| CaptureError::StreamBuild(format!("default_input_config: {e}")))?;

        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let max_samples = (max_duration.as_secs_f64() * f64::from(sample_rate)).round() as usize
            * usize::from(channels);

        let shared = Arc::new(Shared {
            recording: AtomicBool::new(false),
            buffer: Mutex::new(BufferState {
                samples: Vec::new(),
                first_sample_at: None,
                started_at: None,
                truncated: false,
            }),
            max_samples,
            stream_failed: AtomicBool::new(false),
        });

        // cpal 0.18: build_input_stream takes StreamConfig by value and
        // a trailing Option<Duration> timeout for backend init.
        let config = supported.config();
        let err_fn = {
            let s = Arc::clone(&shared);
            move |e: cpal::Error| {
                s.stream_failed.store(true, Ordering::Release);
                warn!("audio input stream runtime error: {e}");
            }
        };
        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let s = Arc::clone(&shared);
                device.build_input_stream::<f32, _, _>(
                    config,
                    move |data, _| push_samples(&s, data.iter().copied()),
                    err_fn.clone(),
                    None,
                )
            }
            SampleFormat::I16 => {
                let s = Arc::clone(&shared);
                device.build_input_stream::<i16, _, _>(
                    config,
                    move |data, _| {
                        push_samples(&s, data.iter().map(|&v| f32::from(v) / 32768.0))
                    },
                    err_fn.clone(),
                    None,
                )
            }
            SampleFormat::U16 => {
                let s = Arc::clone(&shared);
                device.build_input_stream::<u16, _, _>(
                    config,
                    move |data, _| {
                        push_samples(&s, data.iter().map(|&v| (f32::from(v) - 32768.0) / 32768.0))
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                return Err(CaptureError::StreamBuild(format!(
                    "unsupported sample format: {other:?}"
                )))
            }
        }
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

        stream
            .play()
            .map_err(|e| CaptureError::StreamPlay(e.to_string()))?;
        info!(sample_rate, channels, device = %device_name, max_secs = max_duration.as_secs(), "AudioCapturePrewarmed");

        Ok(Self {
            _stream: stream,
            shared,
            sample_rate,
            channels,
            device_name,
        })
    }

    /// Begin accumulating samples: clears the buffer, timestamps the
    /// start and flips the recording flag on.
    pub fn start_buffer(&self) {
        {
            let mut buf = lock_buffer(&self.shared);
            buf.samples.clear();
            buf.first_sample_at = None;
            buf.truncated = false;
            buf.started_at = Some(Instant::now());
        }
        self.shared.recording.store(true, Ordering::Release);
        debug!("audio buffering started");
    }

    /// Stop accumulating and convert the buffered native audio to
    /// 16 kHz mono i16 PCM (downmix -> resample -> quantize).
    pub fn stop_buffer(&self) -> CaptureResult {
        self.shared.recording.store(false, Ordering::Release);
        let (samples, truncated, started_at) = {
            let mut buf = lock_buffer(&self.shared);
            (
                std::mem::take(&mut buf.samples),
                buf.truncated,
                buf.started_at.take(),
            )
        };

        let frames = samples.len() / usize::from(self.channels.max(1));
        let duration =
            Duration::from_secs_f64(frames as f64 / f64::from(self.sample_rate.max(1)));
        if let Some(t0) = started_at {
            debug!(
                wall_ms = t0.elapsed().as_millis() as u64,
                audio_ms = duration.as_millis() as u64,
                truncated,
                "audio buffering stopped"
            );
        }

        let mono = downmix_to_mono(&samples, self.channels);
        let resampled = resample_to_16k(&mono, self.sample_rate);
        let pcm = f32_to_i16(&resampled);
        debug_assert!(TARGET_SAMPLE_RATE == 16_000);
        CaptureResult {
            pcm,
            duration,
            truncated,
        }
    }

    /// When the first stream callback delivered samples after the last
    /// [`Self::start_buffer`], if it happened yet. NFR-02 harness hook.
    pub fn first_sample_instant(&self) -> Option<Instant> {
        lock_buffer(&self.shared).first_sample_at
    }

    /// True if the capture in progress already hit the cap (FR-02)
    /// and is dropping samples. The orchestrator polls this while
    /// Recording to cut proactively at the cap (TD-007). The flag is
    /// cleared by [`Self::start_buffer`] and persists after
    /// [`Self::stop_buffer`] until the next capture begins.
    pub fn is_truncated(&self) -> bool {
        buffer_truncated(&self.shared)
    }

    /// Name of the input device this stream was built on (NFR-11).
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// True while a capture is accumulating samples (NFR-11 watcher:
    /// a recording capture must never be swapped out).
    pub fn is_recording(&self) -> bool {
        self.shared.recording.load(Ordering::Acquire)
    }

    /// True once the stream error-callback fired (device unplugged or
    /// backend failure). The buffered samples remain retrievable via
    /// [`Self::stop_buffer`] - NFR-11 partial-transcription guarantee.
    pub fn is_failed(&self) -> bool {
        self.shared.stream_failed.load(Ordering::Acquire)
    }

    /// Native sample rate the device is captured at.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Native channel count the device is captured at.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

#[cfg(test)]
mod unit_tests {
    //! Device-free tests of the shared-buffer cap logic (TD-007).

    use super::*;

    fn shared_with_cap(max_samples: usize) -> Shared {
        Shared {
            recording: AtomicBool::new(true),
            buffer: Mutex::new(BufferState {
                samples: Vec::new(),
                first_sample_at: None,
                started_at: None,
                truncated: false,
            }),
            max_samples,
            stream_failed: AtomicBool::new(false),
        }
    }

    #[test]
    fn push_within_cap_is_not_truncated() {
        let shared = shared_with_cap(8);
        push_samples(&shared, [0.0f32; 4].into_iter());
        assert!(!buffer_truncated(&shared));
        assert_eq!(lock_buffer(&shared).samples.len(), 4);
    }

    #[test]
    fn push_past_cap_truncates_and_clamps() {
        let shared = shared_with_cap(4);
        push_samples(&shared, [0.0f32; 6].into_iter());
        assert!(buffer_truncated(&shared));
        assert_eq!(lock_buffer(&shared).samples.len(), 4);
    }

    // --- NFR-11 device-watcher decision policy (device-free) ---

    #[test]
    fn watch_keeps_when_nothing_changed() {
        assert_eq!(device_watch_action(Some("Mic A"), "Mic A", false, false), WatchAction::Keep);
    }

    #[test]
    fn watch_rebuilds_on_default_change_while_idle() {
        assert_eq!(device_watch_action(Some("Mic B"), "Mic A", false, false), WatchAction::Rebuild);
    }

    #[test]
    fn watch_rebuilds_on_failed_stream_while_idle() {
        assert_eq!(device_watch_action(Some("Mic A"), "Mic A", false, true), WatchAction::Rebuild);
    }

    #[test]
    fn watch_defers_any_change_while_recording() {
        assert_eq!(device_watch_action(Some("Mic B"), "Mic A", true, false), WatchAction::Defer);
        assert_eq!(device_watch_action(Some("Mic A"), "Mic A", true, true), WatchAction::Defer);
    }

    #[test]
    fn watch_without_default_device_never_rebuilds() {
        // No device to rebuild onto: a healthy stream keeps running, a
        // failed stream defers until a device appears.
        assert_eq!(device_watch_action(None, "Mic A", false, false), WatchAction::Keep);
        assert_eq!(device_watch_action(None, "Mic A", false, true), WatchAction::Defer);
    }
}

#[cfg(test)]
mod integration_tests {
    //! Real-microphone integration tests (plan FASE 2, Task 4).
    //!
    //! These need a working default input device. Run with:
    //!
    //! ```text
    //! cargo test --lib audio -- --ignored --nocapture --test-threads=1
    //! ```

    use super::*;

    #[test]
    #[ignore = "requires a real default input device"]
    fn capture_two_seconds_produces_expected_pcm() {
        let capture = AudioCapture::prewarm().expect("prewarm should find a microphone");
        println!(
            "device native format: {} Hz, {} ch",
            capture.sample_rate(),
            capture.channels()
        );
        // Let WASAPI settle before measuring.
        std::thread::sleep(Duration::from_millis(300));

        capture.start_buffer();
        std::thread::sleep(Duration::from_secs(2));
        let result = capture.stop_buffer();

        let secs = result.duration.as_secs_f64();
        println!(
            "captured {:.3} s -> {} pcm samples (truncated: {})",
            secs,
            result.pcm.len(),
            result.truncated
        );
        assert!(
            (1.8..=2.5).contains(&secs),
            "duration out of range 1.8..=2.5 s: {secs:.3}"
        );
        let expected = secs * 16_000.0;
        let len = result.pcm.len() as f64;
        assert!(
            (len - expected).abs() <= expected * 0.10,
            "pcm len {len} outside {expected} +/- 10%"
        );
        assert!(!result.truncated, "2 s capture must not hit the 60 s cap");
    }

    #[test]
    #[ignore = "requires a real default input device"]
    fn capture_cap_truncates_and_limits_length() {
        let capture = AudioCapture::prewarm_with_max_duration(Duration::from_secs(1))
            .expect("prewarm should find a microphone");
        std::thread::sleep(Duration::from_millis(300));

        capture.start_buffer();
        std::thread::sleep(Duration::from_secs(2));
        let result = capture.stop_buffer();

        let secs = result.duration.as_secs_f64();
        println!(
            "captured {:.3} s -> {} pcm samples (truncated: {})",
            secs,
            result.pcm.len(),
            result.truncated
        );
        assert!(result.truncated, "capturing 2 s with a 1 s cap must truncate");
        assert!(
            capture.is_truncated(),
            "accessor must still report the hit cap after stop_buffer"
        );
        let len = result.pcm.len() as f64;
        assert!(
            (len - 16_000.0).abs() <= 1_600.0,
            "pcm len {len} outside 16000 +/- 10%"
        );
        assert!(
            (0.9..=1.1).contains(&secs),
            "duration should be ~1 s (the cap): {secs:.3}"
        );
    }
}

#[cfg(test)]
mod latency_bench {
    //! NFR-02 latency harness (plan FASE 2, Task 5): synthetic
    //! Ctrl+Win keydown -> ComboPressed -> start_buffer -> first
    //! accumulated audio sample.
    //!
    //! Lives in the audio module because it consumes the private-ish
    //! [`AudioCapture::first_sample_instant`] hook; the hotkey side is
    //! reached through its public API. WARNING: injects REAL key
    //! events system-wide and needs a microphone. Run focused:
    //!
    //! ```text
    //! cargo test --lib bench_hotkey_to_first_sample -- --ignored --nocapture --test-threads=1
    //! ```

    use super::*;
    use crate::hotkey::{HotkeyEvent, HotkeyMonitor};
    use std::mem::size_of;
    use std::sync::mpsc::channel;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };

    const VK_LCONTROL: u16 = 0xA2;
    const VK_LWIN: u16 = 0x5B;

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
        // SAFETY: `input` is fully initialized and SendInput copies it.
        let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
        assert_eq!(sent, 1, "SendInput failed for vk {vk:#04X}");
    }

    /// Mandatory cleanup guard: releases the injected keys even if an
    /// iteration panics, so Ctrl/Win never stay stuck at OS level.
    struct KeyCleanup;

    impl Drop for KeyCleanup {
        fn drop(&mut self) {
            for vk in [VK_LCONTROL, VK_LWIN] {
                send_key(vk, true);
            }
        }
    }

    /// Nearest-rank percentile over an ascending-sorted slice.
    fn percentile(sorted: &[Duration], pct: f64) -> Duration {
        assert!(!sorted.is_empty());
        let rank = ((pct / 100.0) * sorted.len() as f64).ceil() as usize;
        sorted[rank.clamp(1, sorted.len()) - 1]
    }

    #[test]
    #[ignore = "injects real key events and needs a mic; run focused, --test-threads=1"]
    fn bench_hotkey_to_first_sample() {
        const ITERATIONS: usize = 30;
        const FIRST_SAMPLE_TIMEOUT: Duration = Duration::from_millis(500);

        let capture = AudioCapture::prewarm().expect("prewarm should find a microphone");
        let (tx, rx) = channel();
        let monitor = HotkeyMonitor::start(tx).expect("monitor should start");
        let cleanup = KeyCleanup;
        // Let the WASAPI stream and the hook thread settle.
        std::thread::sleep(Duration::from_millis(300));

        let mut deltas: Vec<Duration> = Vec::with_capacity(ITERATIONS);
        for i in 0..ITERATIONS {
            while rx.try_recv().is_ok() {} // drain stale events

            let t0 = Instant::now();
            send_key(VK_LCONTROL, false);
            send_key(VK_LWIN, false);

            // The hook thread publishes ComboPressed on the second keydown.
            loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(HotkeyEvent::ComboPressed) => break,
                    Ok(_) => continue,
                    Err(e) => panic!("iteration {i}: no ComboPressed: {e}"),
                }
            }
            capture.start_buffer();

            // Busy-poll: Windows sleep granularity (~1.5 ms+) would
            // distort a measurement whose budget is 25 ms.
            let deadline = Instant::now() + FIRST_SAMPLE_TIMEOUT;
            let first = loop {
                if let Some(t) = capture.first_sample_instant() {
                    break t;
                }
                assert!(
                    Instant::now() <= deadline,
                    "iteration {i}: no audio sample within {FIRST_SAMPLE_TIMEOUT:?}"
                );
                std::hint::spin_loop();
            };
            deltas.push(first.duration_since(t0));

            let _ = capture.stop_buffer();
            send_key(VK_LWIN, true);
            send_key(VK_LCONTROL, true);
            while rx.try_recv().is_ok() {} // drain the release events
            std::thread::sleep(Duration::from_millis(50));
        }

        drop(cleanup);
        monitor.stop();

        deltas.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let p50 = percentile(&deltas, 50.0);
        let p95 = percentile(&deltas, 95.0);
        let p99 = percentile(&deltas, 99.0);
        let min = deltas[0];
        let max = deltas[deltas.len() - 1];

        println!();
        println!("NFR-02 harness: hotkey keydown -> first audio sample ({ITERATIONS} iterations)");
        println!();
        println!("| metric | measured | threshold | ok |");
        println!("|--------|----------|-----------|----|");
        println!("| min    | {:8.2} ms |      --   | -- |", ms(min));
        println!(
            "| p50    | {:8.2} ms |   25 ms   | {} |",
            ms(p50),
            if ms(p50) <= 25.0 { "yes" } else { "NO" }
        );
        println!(
            "| p95    | {:8.2} ms |   50 ms   | {} |",
            ms(p95),
            if ms(p95) <= 50.0 { "yes" } else { "NO" }
        );
        println!(
            "| p99    | {:8.2} ms |  100 ms   | {} |",
            ms(p99),
            if ms(p99) <= 100.0 { "yes" } else { "NO" }
        );
        println!("| max    | {:8.2} ms |      --   | -- |", ms(max));
        println!();

        if ms(p50) <= 25.0 && ms(p95) <= 50.0 && ms(p99) <= 100.0 {
            println!("NFR-02 verdict: PASS");
        } else {
            // Plan Task 5: a miss is a red flag to document, not a
            // test failure — so no assert on the thresholds.
            println!("NFR-02 verdict: RED FLAG - thresholds exceeded, document in phase evidence");
        }
    }
}
