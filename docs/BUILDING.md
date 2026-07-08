# Building Breeze

This is the honest toolchain guide. Breeze links a Rust crate (with whisper.cpp + CUDA compiled from source) into a Flutter Windows runner, and that combination has sharp edges. The `scripts\` directory encodes the workarounds; this document explains what each one is for so you can adapt it to your machine.

Read this before your first build. Skipping the environment ritual leads to cryptic CMake, `nvcc`, or PDB failures rather than clean errors.

---

## Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Flutter SDK | Dart SDK ≥ 3.10.1 (see `pubspec.yaml`) | Windows desktop enabled (`flutter config --enable-windows-desktop`). |
| Rust | stable toolchain (`rustup`) | `cargo` and the codegen tool must be on `PATH`. |
| flutter_rust_bridge_codegen | 2.12.0 (matches `flutter_rust_bridge` in `pubspec.yaml`) | Install with `cargo install flutter_rust_bridge_codegen --version 2.12.0`. The generated bindings in `lib/src/rust/` and `rust/src/frb_generated.rs` are committed and **must** stay in sync with this version. |
| Visual Studio 2022+ | with the **Desktop development with C++** workload and **Ninja** | Provides `cl.exe`, the Windows SDK, and `ninja.exe`. |
| CUDA Toolkit | **12.6** (12.4/12.5 also accepted) | See the CUDA note below: the major version matters. |
| LLVM / libclang | any recent | Needed by `bindgen` (whisper-rs). `LIBCLANG_PATH` must point at `…\LLVM\bin`. |
| Inno Setup | 6 | Only needed to build the installer. |

The generated FRB bindings are committed to the repo, so a plain build does **not** require running the codegen; you only regenerate after changing a Rust API signature.

---

## Why CUDA 12.x and not 13

whisper.cpp does not report a CUDA init failure through any return value: it logs a warning and **silently falls back to CPU**. That means a mismatched toolkit produces a build that compiles fine and runs at a fraction of the speed, with no error. Two rules avoid it:

1. **Match the toolkit to the installed driver.** A build compiled against a CUDA version newer than your driver supports will fail `ggml_cuda_init` at runtime and fall back to CPU. Compiling against 12.x also maximizes end-user driver compatibility. The env script prefers 12.6 and falls back through 12.5/12.4 before 13.x.
2. Breeze detects the fallback at model load (see `whisper_engine`) and surfaces it, so a silently-degraded build won't pass unnoticed, but you still want to build against the right toolkit in the first place.

---

## The environment ritual: `scripts\dev-env.ps1`

Dot-source this before any build (`. "$PSScriptRoot\dev-env.ps1"`). It is Windows PowerShell 5.1 (`#Requires -Version 5.1`). Each step exists to work around a specific failure:

1. **Activate the VS environment.** Runs `vcvars64.bat` and imports the resulting environment (`cl.exe`, Windows SDK, Ninja onto `PATH`). Edit the `vcvars64.bat` path if your VS install differs.
2. **Remove `VSINSTALLDIR`.** `cmake-rs` maps it to `-DCMAKE_GENERATOR_INSTANCE`, which the Ninja generator rejects.
3. **Force the Ninja generator** (`CMAKE_GENERATOR = "Ninja"`, clear `CMAKE_GENERATOR_PLATFORM`). The MSBuild generators fail against recent VS toolsets and CUDA.
4. **Ensure `ninja.exe` is on `PATH`** (searches the VS CMake extension folder as a fallback).
5. **CUDA setup.** Picks the first installed toolkit from the 12.6 → 12.5 → 12.4 → 13.x candidate list, sets `CUDA_PATH`, `CUDA_COMPUTE_CAP` (single-arch for fast dev builds, see packaging), and `LIBCLANG_PATH`.
6. **`nvcc -allow-unsupported-compiler`.** Recent MSVC toolsets aren't on NVIDIA's certified-host list for CUDA 12.x, so `host_config.h` aborts with C1189 ("unsupported Microsoft Visual Studio version"). The script sets `NVCC_PREPEND_FLAGS` and `CUDAFLAGS` to `-allow-unsupported-compiler` (the standard llama.cpp/whisper.cpp workaround) when the selected toolkit is 12.x. `NVCC_PREPEND_FLAGS` reaches *every* `nvcc` invocation, including CMake's compiler-detection step; `CUDAFLAGS` seeds `CMAKE_CUDA_FLAGS` as a backup.
7. **The MAX_PATH / PDB workaround.** This is the subtle one. Under cargokit, the default build path (`<project>\build\windows\x64\plugins\…\cargokit_build\…`) produces PDB paths longer than `MAX_PATH` (260 chars), and `mspdbsrv` does not support long paths: you get `C1041 "cannot open program database"` in whisper.cpp's `TryCompile` steps. Two defenses:
   - `CFLAGS` / `CXXFLAGS = "/FS"` serializes PDB writes through `mspdbsrv`.
   - `CARGOKIT_TEMP_DIR_OVERRIDE = "C:\dev\ivb"` redirects the build into a short path. The **patched `rust_builder/cargokit/cmake/cargokit.cmake`** honors this override; that patch is vendored and is part of the build story. Point the override at any short path on your machine.
8. **PDB-server isolation.** See `.cargo/config.toml`: `_MSPDBSRV_ENDPOINT_ = "inputvoice-nested-build"` with `force = true`. The outer MSBuild that drives `flutter build windows` exports its own `_MSPDBSRV_ENDPOINT_`; the inner `cl.exe` launched by whisper.cpp's CMake would inherit that foreign endpoint and fail with C1041. Forcing our own endpoint isolates the nested C/C++ build.
9. **Cargo `PATH`.** Adds `~/.cargo/bin` (where `flutter_rust_bridge_codegen` lives).

If you moved Visual Studio, CUDA, or LLVM, the paths in `dev-env.ps1` are the first thing to update.

---

## Dev build (single-arch, fast)

```powershell
. "scripts\dev-env.ps1"
flutter build windows --release
```

or use the wrapper `scripts\build-release.ps1` (dot-sources `dev-env.ps1`, runs `cargo build --release` then `flutter build windows --release`, and verifies `rust_lib_inputvoice.dll` landed in the bundle). `scripts\build-debug.ps1` is the debug equivalent.

The dev build fixes `CUDA_COMPUTE_CAP=89` (Ada / RTX 40-series), compiling a **single** GPU architecture so the dev cycle is fast. The resulting DLL only runs on that architecture: fine for local development, wrong for distribution.

---

## Packaging build (multi-arch fatbin)

```powershell
. "scripts\package-env.ps1"
flutter build windows --release
```

`package-env.ps1` dot-sources `dev-env.ps1` (inheriting all of the above) and then overrides the architecture: it sets `CUDAARCHS = '75-real;80-real;86-real;89-real;90'`, which CMake ≥ 3.20 turns into `CMAKE_CUDA_ARCHITECTURES`. This produces a multi-architecture fatbin covering every NVIDIA GPU with compute capability ≥ 7.5:

- `75` Turing (RTX 20xx, GTX 16xx), `80` Ampere (A100), `86` Ampere (RTX 30xx), `89` Ada (RTX 40xx): `-real`, SASS only, fast startup.
- `90` Hopper: SASS **and** PTX; the PTX gives forward-compatibility via driver JIT on future architectures (Blackwell and later), at the cost of a one-time JIT on first launch on those GPUs.

Note: `whisper-rs-sys` does not consume `CUDA_COMPUTE_CAP`, so the multi-arch selection must go through `CUDAARCHS`. Expect this build to take **substantially longer** than the dev build (five architectures instead of one) and to produce a much larger native DLL (on the order of ~585 MB).

---

## The Whisper model

Breeze needs `ggml-large-v3-turbo-q5_0.bin`:

- Size: 574,041,195 bytes
- SHA-256: `394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2`

Download it from <https://huggingface.co/ggerganov/whisper.cpp> and place it in a `models\` folder next to the built executable (`build\windows\x64\runner\Release\models\`). Breeze checks the size on every start and the full SHA-256 on first run.

---

## Running the tests

Rust tests need the environment ritual (they compile the same native code):

```powershell
. "scripts\dev-env.ps1"
cd rust
cargo test
```

Flutter tests do not:

```powershell
flutter test
```

`scripts\test-all.ps1` runs both in sequence. Note that some hotkey and injection tests inject **real** system-wide key events and are marked `#[ignore]`; run them focused, with hands off the keyboard, and `--test-threads=1` (see the `#[ignore]` messages in the source).

Audio test fixtures are generated by `test-fixtures\generate-fixtures.ps1` (SAPI TTS, Windows PowerShell 5.1 only: `System.Speech` is a .NET Framework assembly and is absent from PowerShell 7). Pre-generated `.wav` fixtures are committed.

---

## Building the installer (Inno Setup 6)

```powershell
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' installer\breeze.iss
```

`installer\breeze.iss` bundles `breeze.exe`, the release DLLs, the CUDA runtime DLLs (`cudart64_12`, `cublas64_12`, `cublasLt64_12`), the **app-local VC++ CRT** (`msvcp140`, `vcruntime140`, `vcruntime140_1`), and the model, so the installer runs on a clean machine with no separate CUDA or VC++ redistributable.

Two source paths in the `.iss` almost certainly need re-pointing to your machine:

- `CudaBin`: your CUDA `bin` directory.
- `VcRedist`: your installed VC++ redistributable version folder (`…\VC\Redist\MSVC\<version>\x64\Microsoft.VC*.CRT`). The version number moves with every VS update; use `dumpbin /dependents breeze.exe` to confirm which CRT DLLs the exe actually imports.

The installer output lands in `installer\output\` (git-ignored).
