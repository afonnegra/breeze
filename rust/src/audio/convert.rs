//! Pure audio conversion functions (FR-02).
//!
//! Downmix interleaved multi-channel f32 frames to mono (channel
//! average), resample from the device's native rate to 16 kHz with
//! rubato (windowed sinc), and convert f32 samples to i16 PCM with
//! clamping. No devices, no streams — fully unit-testable.
//!
//! # rubato 3.0 API note
//!
//! rubato 3.0 removed the 2.x `SincFixedIn` type. The equivalent is
//! [`rubato::Async::new_sinc`] with [`rubato::FixedAsync::Input`], and
//! buffers go through `audioadapter` adapters (re-exported by rubato).
//! For whole-buffer (non-streaming) use, 3.0 provides
//! [`rubato::Resampler::process_all_into_buffer`], which internally
//! runs the chunk loop, the final partial chunk and the flush, and
//! trims the resampler delay — so we use that direct path instead of
//! hand-rolling the chunk+flush loop the 2.x API required.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use tracing::warn;

/// Target sample rate expected by `WhisperEngine::transcribe`.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Input chunk size (frames) for the resampler. One chunk at 48 kHz is
/// ~21 ms of audio; conversion runs off the hot path (at stop), so the
/// exact value only trades memory for loop iterations.
const RESAMPLER_CHUNK_FRAMES: usize = 1024;

/// Downmix interleaved `frames` to mono by averaging the channels of
/// each frame.
///
/// - `channels == 1` is a passthrough copy.
/// - `channels == 0` returns an empty vector: zero channels cannot
///   describe any frame layout, so there is no meaningful output.
/// - A trailing partial frame (input length not divisible by
///   `channels`, i.e. malformed input) is dropped.
pub fn downmix_to_mono(frames: &[f32], channels: u16) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => frames.to_vec(),
        _ => {
            let ch = usize::from(channels);
            frames
                .chunks_exact(ch)
                .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                .collect()
        }
    }
}

/// Resample mono f32 samples from `src_rate` to 16 kHz using rubato's
/// windowed-sinc `Async` resampler (see module docs for the 3.0 API
/// rationale).
///
/// - `src_rate == 16_000` is a passthrough copy.
/// - Empty input, or the nonsensical `src_rate == 0`, returns an empty
///   vector.
/// - Internal resampler failures are logged and yield an empty vector;
///   they cannot happen with the fixed parameters below and correctly
///   sized buffers, but the signature has no error channel (plan API).
pub fn resample_to_16k(mono: &[f32], src_rate: u32) -> Vec<f32> {
    if mono.is_empty() || src_rate == 0 {
        return Vec::new();
    }
    if src_rate == TARGET_SAMPLE_RATE {
        return mono.to_vec();
    }

    let ratio = f64::from(TARGET_SAMPLE_RATE) / f64::from(src_rate);
    // Values from rubato's own guidance: sinc_len 256 / f_cutoff 0.95 /
    // oversampling 128 are the documented starting points for good
    // quality; cubic interpolation is the best quality/cost trade-off.
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Cubic,
        window: WindowFunction::BlackmanHarris2,
    };
    // The ratio is fixed for the life of this one-shot resampler, so
    // the allowed relative adjustment range is irrelevant; 1.1 is the
    // smallest sensible value.
    let mut resampler = match Async::<f32>::new_sinc(
        ratio,
        1.1,
        &params,
        RESAMPLER_CHUNK_FRAMES,
        1,
        FixedAsync::Input,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("resampler construction failed ({src_rate} -> 16k): {e}");
            return Vec::new();
        }
    };

    let needed = resampler.process_all_needed_output_len(mono.len());
    let mut out = vec![0.0f32; needed];

    let input = match InterleavedSlice::new(mono, 1, mono.len()) {
        Ok(adapter) => adapter,
        Err(e) => {
            warn!("resampler input adapter failed: {e}");
            return Vec::new();
        }
    };
    let mut output = match InterleavedSlice::new_mut(&mut out, 1, needed) {
        Ok(adapter) => adapter,
        Err(e) => {
            warn!("resampler output adapter failed: {e}");
            return Vec::new();
        }
    };

    match resampler.process_all_into_buffer(&input, &mut output, mono.len(), None) {
        Ok((_consumed, produced)) => {
            out.truncate(produced);
            out
        }
        Err(e) => {
            warn!("resampling {src_rate} -> 16k failed: {e}");
            Vec::new()
        }
    }
}

/// Convert normalized f32 samples to i16 PCM: clamp to [-1.0, 1.0] and
/// scale by 32768 (the exact inverse of
/// [`crate::whisper_engine::pcm_i16_to_f32`]). +1.0 saturates to
/// `i16::MAX` (32768 is not representable).
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| {
            let scaled = (s.clamp(-1.0, 1.0) * 32768.0).round() as i32;
            scaled.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
        })
        .collect()
}

/// Root-mean-square energy of 16-bit PCM, normalized to [0.0, 1.0]
/// (each sample divided by 32768 before squaring). Empty input is 0.0.
///
/// Consumed by the orchestrator silence gate (TD-009). whisper
/// hallucinates plausible text when fed silence, so captures whose
/// RMS is below a calibrated threshold are discarded before the
/// engine ever runs.
pub fn rms_energy(pcm: &[i16]) -> f64 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = pcm
        .iter()
        .map(|&s| {
            let x = f64::from(s) / 32768.0;
            x * x
        })
        .sum();
    (sum_sq / pcm.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// Generate a mono sine wave.
    fn sine(freq: f32, rate: u32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amplitude * (TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Duplicate a mono signal into interleaved stereo (L == R).
    fn interleave_identical_stereo(mono: &[f32]) -> Vec<f32> {
        mono.iter().flat_map(|&s| [s, s]).collect()
    }

    // ── downmix_to_mono ─────────────────────────────────────────────

    #[test]
    fn downmix_stereo_identical_channels_equals_original() {
        let mono = sine(440.0, 48_000, 0.1, 0.5);
        let stereo = interleave_identical_stereo(&mono);
        let out = downmix_to_mono(&stereo, 2);
        assert_eq!(out.len(), mono.len());
        for (i, (a, b)) in out.iter().zip(mono.iter()).enumerate() {
            assert!((a - b).abs() < 1e-6, "sample {i}: {a} != {b}");
        }
    }

    #[test]
    fn downmix_averages_differing_channels() {
        // Frames: (1.0, 0.0) and (0.5, -0.5) -> averages 0.5 and 0.0.
        let frames = [1.0, 0.0, 0.5, -0.5];
        assert_eq!(downmix_to_mono(&frames, 2), vec![0.5, 0.0]);
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let frames = [0.25, -0.75, 0.0];
        assert_eq!(downmix_to_mono(&frames, 1), frames.to_vec());
    }

    #[test]
    fn downmix_zero_channels_returns_empty() {
        assert!(downmix_to_mono(&[0.1, 0.2], 0).is_empty());
    }

    #[test]
    fn downmix_empty_returns_empty() {
        assert!(downmix_to_mono(&[], 2).is_empty());
    }

    // ── resample_to_16k ─────────────────────────────────────────────

    #[test]
    fn resample_48k_sine_has_expected_length_and_energy() {
        // 1 s of 440 Hz at 48 kHz stereo, downmixed then resampled.
        let mono = sine(440.0, 48_000, 1.0, 0.5);
        let stereo = interleave_identical_stereo(&mono);
        let mixed = downmix_to_mono(&stereo, 2);
        let out = resample_to_16k(&mixed, 48_000);

        let expected = mixed.len() as f64 * 16_000.0 / 48_000.0;
        let tolerance = expected * 0.02;
        assert!(
            (out.len() as f64 - expected).abs() <= tolerance,
            "resampled len {} outside {expected} +/- {tolerance}",
            out.len()
        );
        // A 0.5-amplitude sine has RMS ~0.354; assert it survived.
        assert!(rms(&out) > 0.1, "RMS too low: {}", rms(&out));
    }

    #[test]
    fn resample_16k_is_passthrough() {
        let mono = sine(440.0, 16_000, 0.25, 0.5);
        assert_eq!(resample_to_16k(&mono, 16_000), mono);
    }

    #[test]
    fn resample_empty_returns_empty() {
        assert!(resample_to_16k(&[], 48_000).is_empty());
    }

    #[test]
    fn resample_zero_rate_returns_empty() {
        assert!(resample_to_16k(&[0.1, 0.2], 0).is_empty());
    }

    // ── f32_to_i16 ──────────────────────────────────────────────────

    #[test]
    fn f32_to_i16_clamps_out_of_range() {
        // +/-2.0 must clamp to +/-1.0 first, then scale.
        assert_eq!(f32_to_i16(&[2.0, -2.0]), vec![i16::MAX, i16::MIN]);
    }

    #[test]
    fn f32_to_i16_is_inverse_of_pcm_i16_to_f32() {
        let original: Vec<i16> = vec![i16::MIN, -1234, -1, 0, 1, 5678, i16::MAX];
        let as_f32 = crate::whisper_engine::pcm_i16_to_f32(&original);
        assert_eq!(f32_to_i16(&as_f32), original);
    }

    #[test]
    fn f32_to_i16_empty_returns_empty() {
        assert!(f32_to_i16(&[]).is_empty());
    }

    // ── rms_energy (TD-009) ────────────────────────────────────────

    #[test]
    fn rms_energy_empty_is_zero() {
        assert_eq!(rms_energy(&[]), 0.0);
    }

    #[test]
    fn rms_energy_absolute_silence_is_zero() {
        assert_eq!(rms_energy(&vec![0i16; 16_000]), 0.0);
    }

    #[test]
    fn rms_energy_full_scale_sine_is_about_0707() {
        // 440 Hz at 16 kHz over 16 000 samples = 440 exact cycles.
        let pcm: Vec<i16> = (0..16_000)
            .map(|i| {
                let x = (std::f64::consts::TAU * 440.0 * f64::from(i) / 16_000.0).sin();
                (x * 32_767.0).round() as i16
            })
            .collect();
        let rms = rms_energy(&pcm);
        assert!((rms - 0.707).abs() < 0.01, "expected ~0.707, got {rms}");
    }

    #[test]
    fn rms_energy_half_scale_dc_is_half() {
        // Constant 16 384 = exactly half of the 32 768 normalization.
        let rms = rms_energy(&vec![16_384i16; 1_000]);
        assert!((rms - 0.5).abs() < 1e-9, "expected 0.5, got {rms}");
    }
}
