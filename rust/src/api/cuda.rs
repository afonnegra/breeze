//! Real CUDA availability check (TD-003).
//!
//! Phase 0 shipped this as a stub hardcoded to `true` — which is exactly how
//! the silent CPU fallback of TD-003 went unnoticed (whisper.cpp downgraded
//! to CPU without any error surfacing). The check is now real, two-tiered:
//!
//! 1. If a `WhisperEngine` has been loaded in this process, report the
//!    backend whisper.cpp *actually* initialized (evidence taken from its
//!    log stream — see `crate::whisper_engine::cuda_runtime_active`).
//! 2. Otherwise probe the NVIDIA driver directly: load `nvcuda.dll` and call
//!    `cuInit(0)`. This is the same driver API ggml's CUDA backend uses, so
//!    it accurately predicts whether GPU init will succeed (e.g. it fails on
//!    the driver/toolkit version mismatch of TD-003) without allocating VRAM
//!    or requiring the model.

use std::sync::OnceLock;

pub fn cuda_available() -> bool {
    match crate::whisper_engine::cuda_runtime_active() {
        Some(active) => active,
        None => *DRIVER_PROBE.get_or_init(probe_nvcuda),
    }
}

/// `cuInit` is idempotent and its outcome cannot change within a process
/// lifetime (driver install/uninstall requires at least a session restart),
/// so the probe runs at most once per process.
static DRIVER_PROBE: OnceLock<bool> = OnceLock::new();

fn probe_nvcuda() -> bool {
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    // SAFETY: nvcuda.dll is the CUDA entry point installed in System32 by
    // the NVIDIA driver — if it is missing there is no CUDA. `cuInit` has
    // the documented signature `CUresult cuInit(unsigned int Flags)`
    // (CUDAAPI == __stdcall, which on x64 is the one native convention);
    // 0 == CUDA_SUCCESS. The module handle is intentionally never freed:
    // the probe result is cached for the process lifetime.
    unsafe {
        let Ok(module) = LoadLibraryW(w!("nvcuda.dll")) else {
            return false;
        };
        let Some(cu_init_ptr) = GetProcAddress(module, s!("cuInit")) else {
            return false;
        };
        type CuInitFn = unsafe extern "system" fn(flags: u32) -> i32;
        let cu_init =
            std::mem::transmute::<unsafe extern "system" fn() -> isize, CuInitFn>(cu_init_ptr);
        cu_init(0) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuda_available_is_stable() {
        // The concrete value depends on the machine (GPU or not), so assert
        // the contract instead: callable without panicking and deterministic
        // within a process.
        assert_eq!(cuda_available(), cuda_available());
    }

    #[test]
    fn driver_probe_does_not_crash() {
        // Exercises the raw FFI path directly (bypassing the OnceLock cache):
        // must return a bool on any machine, never crash.
        let _ = probe_nvcuda();
    }
}
