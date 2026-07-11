# Breeze

**Fully offline, GPU-accelerated push-to-talk dictation for Windows.**

[![License: PolyForm Noncommercial](https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-blue.svg)](LICENSE)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11%20x64-0078D6.svg)](#requirements)
[![Built with Rust + Flutter](https://img.shields.io/badge/Built%20with-Rust%20%2B%20Flutter-DE4A16.svg)](#how-it-works)
[![Latest release](https://img.shields.io/github/v/release/afonnegra/breeze)](https://github.com/afonnegra/breeze/releases/latest)

Hold `Ctrl`+`Win`, speak, release. The text appears where your cursor already is, in any application. No cloud, no account, no network. Your voice is transcribed by Whisper running on your own NVIDIA GPU and never leaves the machine.

**[Download the latest installer](https://github.com/afonnegra/breeze/releases/latest)** (self-contained, about 833 MB: app, Whisper model, CUDA and VC++ runtimes included).

---

## Why Breeze

- **Private by design.** There is no network code in the transcription path. Audio is captured, converted, and transcribed entirely in-process on your GPU. Nothing is uploaded, nothing is logged: the transcript is never written to disk and the last transcription is cleared from memory on exit.
- **Fast.** Push-to-talk with instant capture. Median hold-release-to-paste latency is ~430 ms on the reference machine: you release the keys and the text is already landing.
- **Works in any app.** Text is delivered through a clipboard-preserving paste injection, so it drops into editors, browsers, chat apps, terminals, and IDEs alike, anywhere that accepts `Ctrl+V`. Whatever you had on the clipboard is snapshotted and restored afterwards.

## Features

- **Push-to-talk hotkey**: hold `Ctrl`+`Win` to dictate; release to transcribe and paste. A third key or a session lock cancels the capture cleanly.
- **Whisper large-v3-turbo (q5_0 quantized)** running on CUDA, resident in VRAM for instant reuse.
- **Bilingual dictation**: Spanish and English.
- **Bilingual UI**: English and Spanish interface, switchable at runtime.
- **System tray control**: pause/resume, dictation language, UI language, on-demand model verification, open logs folder, quit.
- **Theme-aware tray icons**: icons follow your taskbar's light/dark theme automatically.
- **Hot microphone switching**: the default input device can change while the app runs; capture rebuilds onto the new device without a restart.
- **Clipboard preservation**: text, images (DIB), and file lists (HDROP) on your clipboard survive a dictation.
- **Single instance**: a named mutex prevents a second launch from double-installing the keyboard hook.
- **Session-lock safety**: locking Windows (`Win`+`L`) mid-hold force-releases the combo so dictation never gets stuck open.
- **Measured footprint**: ~430 ms median hold-release-to-paste latency; ~0.75% GPU and ~390 MB RAM at idle on the reference machine.

## How it works

Breeze is a Flutter (Dart) UI shell over a Rust core linked as a native library via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge). The Rust side owns every latency-sensitive path.

```
   Ctrl+Win held                              Ctrl+Win released
        │                                            │
        ▼                                            ▼
 ┌───────────────┐   combo    ┌────────────────────────────────────┐
 │ WH_KEYBOARD_LL│──────────▶│         Orchestrator (state machine) │
 │  global hook  │  events    └───────────────┬────────────────────┘
 └───────────────┘                            │
        pre-warmed WASAPI capture (always on) │ start / stop
                                              ▼
 ┌──────────────────────────────────────────────────────────┐
 │  native-format audio  ─▶  downmix mono  ─▶  resample 16k  │
 │                        ─▶  f32 → i16 PCM  ─▶  RMS gate     │
 └───────────────────────────────┬──────────────────────────┘
                                 ▼
                     ┌──────────────────────┐
                     │ whisper.cpp on CUDA  │
                     └───────────┬──────────┘
                                 ▼ text
 ┌──────────────────────────────────────────────────────────┐
 │ snapshot clipboard ─▶ set text ─▶ SendInput Ctrl+V (marked)│
 │ ─▶ settle ─▶ restore clipboard                            │
 └──────────────────────────────────────────────────────────┘
```

Key engineering decisions:

- **The capture stream is always running.** WASAPI shared-mode initialization is slow, so the microphone stream is pre-warmed once at startup and left playing. Pressing the hotkey only flips an atomic flag to start accumulating samples; the hotkey-to-first-sample path never waits on device init.
- **Native-format capture, converted at stop time.** WASAPI serves its mix format (typically 44.1/48 kHz float stereo). Breeze captures that and downmixes + resamples to 16 kHz mono i16 only when the hold ends, rather than forcing an exclusive-mode 16 kHz stream that many devices reject.
- **RMS silence gate.** whisper.cpp hallucinates plausible-sounding phrases on near-silent input. Captures whose RMS energy falls below a calibrated threshold are treated as empty and never sent to the engine.
- **A watchdog owns the GPU call.** Transcription runs on a worker thread under a timeout; a late result (after the watchdog fires) lands on a closed channel and is discarded, so a stalled GPU call can never paste stale text into a later context.
- **The overlay never steals focus.** The status pill is a layered, click-through, `WS_EX_NOACTIVATE` top-most window shown with `SW_SHOWNOACTIVATE`, so it never pulls focus away from the app you are dictating into.
- **Our own paste is invisible to the hook.** The synthetic `Ctrl+V` is tagged with a marker in `dwExtraInfo` that the keyboard hook recognizes and drops, so injection can't feed back into the combo tracker.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full deep dive.

## Requirements

- Windows 10 or 11, x64.
- NVIDIA GPU with CUDA compute capability **≥ 7.5** (RTX 20-series or newer).
- NVIDIA driver **≥ r525**.
- ~1.8 GB free disk (application + model + CUDA runtime DLLs).

Without a compatible GPU/driver, whisper.cpp silently falls back to CPU. Breeze detects this at model load and surfaces it rather than shipping a silently-degraded experience.

## Install

### From the installer

Download `breeze-setup-1.0.0.exe` from the [latest release](https://github.com/afonnegra/breeze/releases/latest) and run it. The installer is self-contained: it bundles `breeze.exe`, the Rust/CUDA DLLs, the app-local VC++ runtime, and the Whisper model, so it works on a clean machine with no separate installs. It is not code-signed, so Windows SmartScreen will warn on first run: choose "More info", then "Run anyway".

To build the installer yourself instead, use [Inno Setup 6](https://jrsoftware.org/isinfo.php):

```
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' installer\breeze.iss
```

The output lands in `installer\output\`. Building it requires the release bundle and the model in place first; see [docs/BUILDING.md](docs/BUILDING.md).

### Build from source

See [docs/BUILDING.md](docs/BUILDING.md) for the full toolchain (Flutter, Rust, flutter_rust_bridge_codegen, Visual Studio + Ninja, CUDA Toolkit 12.6, LLVM/libclang) and the build ritual.

```
git clone https://github.com/afonnegra/breeze.git
cd breeze
```

### The Whisper model

Breeze uses `ggml-large-v3-turbo-q5_0.bin`:

- **Size:** 574,041,195 bytes
- **SHA-256:** `394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2`

Download it from the whisper.cpp model repository on Hugging Face (<https://huggingface.co/ggerganov/whisper.cpp>) and place it in a `models\` folder next to the executable. Breeze verifies the file size on every start and computes the full SHA-256 on first run; the installer bundles the model for you.

## Usage

1. Position your cursor where you want the text (any text field, in any app).
2. Hold `Ctrl`+`Win`.
3. Speak.
4. Release. The transcription is pasted at the cursor.

The tray icon shows current state (idle / recording / paused). Right-click it for:

- **Pause / Resume**: uninstalls the keyboard hook entirely while paused (zero idle impact); the engine and microphone stay warm so resume is instant.
- **Dictation language**: Spanish or English.
- **UI language**: English or Spanish, applied immediately.
- **Verify model**: re-runs the full SHA-256 integrity check on demand.
- **Open logs folder**: opens the log directory in Explorer.
- **Quit.**

## Privacy

- **No network.** There is no networking code in the capture → transcription → injection path.
- **No telemetry.** Nothing is collected or phoned home.
- **The transcript is never logged.** Logs record phase durations and text *lengths* for latency instrumentation, never the transcribed content.
- **Last transcription cleared on exit.** In-memory transcription state does not persist across runs.

## License

Breeze is **free for personal and any other noncommercial use** under the [PolyForm Noncommercial License 1.0.0](LICENSE).

**Commercial use requires a paid commercial license.** If you want to use Breeze inside a company, in a product, or in any for-profit setting, open an issue or contact [@afonnegra](https://github.com/afonnegra) to arrange terms.
