# Breeze Architecture

Breeze is a Flutter (Dart) UI shell over a Rust core. The Rust crate is compiled to a native library and linked into the Flutter Windows runner via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) (FRB). Every latency-sensitive and OS-integration concern lives in Rust; Dart owns only the visible UI (the overlay pill and the system tray) and the startup orchestration.

This document describes the real modules and their behavior. It is a map for contributors, not a tutorial.

---

## 1. Process and thread map

A single Breeze process runs the following threads:

| Thread | Owner | Responsibility |
|---|---|---|
| UI isolate | Flutter | The overlay window and tray; listens to the orchestrator's UI-state stream. |
| FRB worker pool | flutter_rust_bridge | Executes `async` Rust API calls dispatched from Dart. |
| Hotkey monitor | `hotkey` | Installs the `WH_KEYBOARD_LL` hook, runs a message pump, owns the hidden session-notification window. |
| Orchestrator | `orchestrator::runtime` | The dictation cycle loop; drives the pure state machine against real modules. |
| Transcribe worker (per cycle) | `orchestrator::runtime` | Runs the blocking GPU transcription under the watchdog; named `transcribe-{cycle_id}`. |
| Device watcher | `audio` / orchestrator | Polls the default input device and rebuilds capture when it changes (NFR-11). |
| Theme watcher | `theme` | Hidden window listening for `WM_SETTINGCHANGE("ImmersiveColorSet")` to track taskbar light/dark. |
| FRB stream bridges | flutter_rust_bridge | Deliver Rust `Stream` events (UI state, hotkey events) to Dart. |

The design principle throughout: **anything the OS calls back into (hook callbacks, WndProcs) runs on a thread that owns a message pump, and its callback context lives in a `thread_local`**, so the callback is lock-free and can never observe half-initialized state.

---

## 2. Rust module tour (`rust/src/`)

### `hotkey/`: global combo detection (FR-01)

- **`state.rs`: `ComboTracker`.** A pure state machine over key events. The combo is *(any Ctrl down) AND (any Win down)*, tracking `LCONTROL`/`RCONTROL` and `LWIN`/`RWIN` independently. It emits `TrackerOutput::ComboPressed`, `ComboReleased { hold, reason }`, or `Nothing`. `ReleaseReason` is one of:
  - `KeyLifted`: a combo key was released.
  - `OtherKeyPressed`: a third key was pressed while the combo was active; the OS owns that shortcut (e.g. `Ctrl+Win+D`), so dictation cancels immediately.
  - `SessionLocked`: the session locked; `reset()` forces the tracker back to idle (key-up events may never arrive after `Win+L`).
  Time is injected via a clock closure, so hold durations are fully unit-testable. Autorepeat key-downs and key-ups of keys that were never down are no-ops.
- **`mod.rs`: `HotkeyMonitor`.** A dedicated thread installs `SetWindowsHookExW(WH_KEYBOARD_LL)` and runs its own `GetMessageW` pump. The hook callback is a plain C function pointer with no context parameter, so its state (`ComboTracker` + event `Sender`) lives in a `thread_local` (`HOOK_STATE`); both the hook and the session WndProc run on this one thread. The callback budget is well under 1 ms: a couple of branches, one pure state-machine step, and one wait-free `mpsc::Sender::send`. Keys are **never consumed**: the callback always defers to `CallNextHookEx`. A process-wide `Mutex<bool>` (not a `OnceLock`, because FR-11 pause/resume needs stop/start cycles) rejects a second monitor. The same thread owns a hidden **normal** top-level window (not `HWND_MESSAGE`, which field reports show can miss `WM_WTSSESSION_CHANGE`) registered via `WTSRegisterSessionNotification` to detect lock/unlock.

### `audio/`: microphone capture (FR-02)

- **`mod.rs`: `AudioCapture`.** Owns a pre-warmed cpal (WASAPI shared-mode) input stream in the device's **native format**. The stream callback always runs; `start_buffer()`/`stop_buffer()` only toggle an `AtomicBool`, so the hotkey → first-sample path never waits on WASAPI init (NFR-02). A 60 s cap stops accumulation and marks the capture truncated. A `WatchAction` enum (`Keep` / `Rebuild` / `Defer`) drives the device watcher: rebuild when the default changes while idle, but never yank a dictation in flight.
- **`convert.rs`: conversion pipeline.** `downmix_to_mono` (averages channels, drops trailing partial frames), `resample_to_16k` (rubato `Async` sinc resampler: `sinc_len=256`, `f_cutoff=0.95`, `oversampling=128`, cubic interpolation, Blackman-Harris window), and `f32_to_i16` (clamp to `[-1,1]`, scale by 32768, round, clamp to `i16`). `rms_energy` returns normalized RMS in `[0,1]` for the orchestrator's silence gate.

### `whisper_engine/`: GPU transcription (FR-04)

- **`mod.rs`: `WhisperEngine`.** A `WhisperContext` is created once in `load` and stays resident in VRAM; each `transcribe` creates its own `WhisperState`. A `Mutex<()>` with `try_lock` guarantees at most one transcription in flight; a second concurrent call returns `EngineError::Busy`. There is deliberately **no internal timeout** (a timeout over a blocking GPU call cancels nothing and would leave the lock held forever); the watchdog lives in the orchestrator instead.
- **Silent-fallback detection.** whisper.cpp does not report CUDA init failure through any return value: it logs and silently falls back to CPU. Breeze installs its own whisper/ggml log callback (via the `raw-api` re-export), classifies the log lines emitted during model load into a `GpuInitOk` / `GpuInitFailed` signal, and records the outcome in process-global state that `api::cuda::cuda_available()` reads. The log callback is wrapped in `catch_unwind` so a Rust panic can never unwind into C.
- `Language` (`Es`/`En`) maps to whisper language codes `"es"`/`"en"`.

### `clipboard/`: snapshot / restore (FR-06)

- **`mod.rs`: `ClipboardManager`.** All access goes through a RAII guard: open with retry + backoff (5 attempts, 10 ms base), close guaranteed on drop. `ClipboardSnapshot` captures `CF_UNICODETEXT`, `CF_DIB`, and `CF_HDROP` as opaque `HGLOBAL` byte blobs, enough to round-trip text, images, and file lists. `restore()` re-inserts them in the same order.

### `inject/`: paste injection (FR-05)

- **`mod.rs`: `TextInjector`.** The full sequence: (1) wait for physical modifiers (Ctrl, Win, Shift, Alt) to release, polling every 15 ms up to a 2 s timeout: a synthetic `Ctrl+V` fired while `Win` is still down becomes `Ctrl+Win+V` (clipboard history popup); (2) snapshot the user's clipboard; (3) set the transcription as `CF_UNICODETEXT`; (4) synthesize `Ctrl+V` as four key events (`Ctrl↓ V↓ V↑ Ctrl↑`) in one `SendInput` call, each tagged with `INJECT_MARKER` in `dwExtraInfo`; (5) wait `PASTE_SETTLE_MS` (300 ms) for the target app to consume the paste; (6) restore the previous clipboard. **The marker trick:** the keyboard hook checks `is_own_injection(dwExtraInfo)` and drops marked events *before* the combo tracker sees them, so our own paste can't cancel or re-trigger the combo. Outcomes are `Pasted` or `PastedRestoreFailed` (paste happened, restore failed, not fatal). A static `INJECT_LOCK` serializes `inject()` calls, since it mutates system-global state (clipboard, synthetic input). Injection returns a `to_paste` duration measured from entry to the `Ctrl+V` dispatch (the paste-landed moment for the NFR-01 latency metric), excluding the settle + restore tail.

### `orchestrator/`: the dictation cycle (FASE 4)

Split into two layers so the transition table is unit-testable without a GPU, mic, or hook.

- **`logic.rs`: pure state machine.** Maps `(state, input)` to a new state plus an ordered list of `Command`s, with zero I/O. States: `Idle`, `Recording`, `Transcribing`, `Injecting`. Inputs: `ComboPressed`, `ComboReleased { hold_ms }`, `SessionLocked`, `CapReached`, `TranscriptReady(String)`, `TranscriptFailed(String)`, `InjectFinished(bool)`, `InjectFailed(String)`. Commands: `StartCapture`, `StopCaptureAndTranscribe`, `DiscardCapture`, `Inject(String)`, `EmitState(UiState)` (where `UiState` ∈ `Listening`/`Transcribing`/`Injecting`/`Error`/`Hidden`). The robustness rules are the interesting part: a `ComboPressed` mid-cycle is ignored (never re-enter); a `TranscriptReady` outside `Transcribing` is ignored (the watchdog already moved on, so a late result must never paste); a sub-200 ms release is discarded silently as an accidental press; an empty transcript ends the cycle as success with no paste; `SessionLocked` aborts only while `Recording`.
- **`runtime.rs`: the thread.** One dedicated thread blocks on `hotkey_rx.recv_timeout(250 ms)`. On timeout while recording it polls the capture's truncated flag and feeds `CapReached` if the 60 s cap hit. On stop it computes RMS and applies the **silence gate** (below the threshold ⇒ treated as empty), then spawns a per-cycle worker thread for the blocking GPU call and waits on a fresh `mpsc::channel` under a **watchdog** (15 s). If the watchdog fires, the receiver is dropped; the late worker result then fails to send and dies in the worker thread, so it can never paste. After a cycle, hotkey events that accumulated during the blocking wait are **selectively drained**: stale releases echoing this cycle are dropped, but a trailing press with no release (user still holding) is replayed after the cycle settles. A monotonic `cycle_id` correlates every phase log line; only lengths and durations are logged, never transcript text (NFR-12).

### `config/`: persistence (FR-10)

- **`mod.rs`: `AppConfig`.** Fields: `language`, `paused`, `log_level`, `ui_language`, `model_verified`. The **conservative merge** validates each field individually: one invalid field never discards the valid ones, and corrupt JSON yields pure defaults (unsupported dictation language falls back to `"es"`, UI language to `"en"`). Saves are **atomic**: write to `config.json.tmp`, then `std::fs::rename` over the target (atomic on the same volume), so a crash mid-write leaves either the old or the new file, never a truncated mix.

### `model/`: locate and verify (FR-09)

- **`mod.rs`.** Constants: filename `ggml-large-v3-turbo-q5_0.bin`, size `574,041,195` bytes, SHA-256 `394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2`. `locate_model` searches the given dirs in order; the first file found decides: a size mismatch returns immediately rather than continuing. `verify_sha256` streams the file in 1 MB chunks (never loading 574 MB into RAM), expensive (~seconds), so it runs only on first run and on-demand. `default_search_dirs` = the exe directory plus `%LOCALAPPDATA%\Breeze\models`.

### `theme/`: taskbar theme (TD-014)

- **`mod.rs`: `ThemeWatcher`.** Tray icons must contrast with the taskbar, whose color follows `HKCU\...\Themes\Personalize\SystemUsesLightTheme` (not `AppsUseLightTheme`). `system_uses_light_theme()` reads that DWORD, returning `Option<bool>` (fall back to dark, the Windows 11 default). The watcher thread owns a hidden top-level window listening for `WM_SETTINGCHANGE("ImmersiveColorSet")`, re-reads the registry on each broadcast, de-dupes (Windows fires several per toggle), and publishes only real changes.

### `paths.rs`: root resolution (TD-010)

Config and logs must agree on one root even though `dirs::data_dir()` resolves differently under an Explorer double-click versus a terminal launch. `breeze_root()` resolves in priority order: `%APPDATA%` (always set) → Known Folder API → exe-relative `…/Breeze` → current directory.

### `api/`: the FRB surface (`rust/src/api/`)

Each file here is a thin, FRB-exposed boundary over the modules above: `orchestrator.rs`, `transcription.rs`, `input.rs`, `config.rs`, `cuda.rs`, `theme.rs`, `overlay.rs`, `instance.rs`, `winprobe.rs`, and a `simple.rs` bootstrap leftover. Two are worth calling out:

- **`overlay.rs`: window styling and visibility (FR-03).** Rust locates the Flutter runner window by class **and** exact title (a double filter, TD-013, retried up to 3× against the startup race), then OR-s on `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW` and calls `SetLayeredWindowAttributes(alpha=255)` (required for a layered window to render at all). `show_overlay_no_activate` uses `ShowWindow(SW_SHOWNOACTIVATE)` so the overlay never takes focus from the dictation target. The **menu-mode dance** temporarily strips `NOACTIVATE` + `TRANSPARENT`, attaches thread input to the foreground thread, and calls `SetForegroundWindow` so the tray plugin can pop its context menu, then restores the FR-03 styles.
- **`instance.rs`: single-instance guard (TD-011).** Each process installs the LL keyboard hook, so a second launch would capture and paste twice. A session-scoped named mutex (`Local\Breeze-single-instance-mutex`) is created and held for the process lifetime; a second process sees `ERROR_ALREADY_EXISTS` and exits before initializing any subsystem. The check is idempotent (cached in a `OnceLock`) and fail-open (if guard creation fails, startup is allowed).

---

## 3. The Dart side (`lib/`)

- **`main.dart`: startup.** `RustLib.init()` → single-instance check (silent `exit(0)` if another instance owns the guard) → window setup (180×48 frameless pill, bottom-right, always-on-top, off the taskbar; title `"Breeze-overlay"` is the exact-match contract with `overlay.rs`) → `loadConfig` → `initEngine(fullVerify)` (SHA-256 on first run) → `audioPrewarm` → `applyOverlayStyles` → tray init → taskbar-theme watch → if not paused, `attachOrchestrator` and subscribe to the UI-state stream → `hideOverlay`. A fatal startup error enlarges the overlay to a readable panel and shows it (correct here: the app is dead, there's no dictation target to protect).
- **`overlay/overlay_screen.dart`: overlay states.** `OverlayController` is a `ValueNotifier<OverlayPhase>` fed by the orchestrator's `UiStateDto` stream. Phases: `PhaseStarting`, `PhaseHidden`, `PhaseListening`, `PhaseTranscribing`, `PhaseInjecting`, `PhaseError`, `PhaseFatal`. It drives visibility exclusively through the Rust `showOverlayNoActivate` / `hideOverlay` APIs, **never** `windowManager.show()`, which would activate the window. An anti-flicker rule keeps the pill visible for 300 ms after a `Hidden` before actually hiding.
- **`tray/tray_controller.dart`: the tray.** Menu: status line, pause/resume (FR-11), dictation-language submenu (FR-07), UI-language submenu (FR-13), verify model (FR-09), open logs, quit. Two pure, testable mappings drive the visuals: `trayStateLabel(phase, paused, strings)` and `trayIconAsset(phase, paused, lightTaskbar)` (idle / recording / paused variants × light/dark suffix). Pause calls `stop_orchestrator`: the LL hook is fully uninstalled (zero idle impact) while the engine and mic stay warm for instant resume; the state is persisted with a merge-style write that never clobbers `model_verified`. On-demand verification stops the orchestrator, persists `paused=true` and `model_verified=false`, and surfaces a fatal overlay on failure, so a broken model can't leave an armed hotkey (I-2). Popping the context menu uses the overlay `enable_menu_mode` / `disable_menu_mode` dance.
- **`l10n/strings.dart`: i18n (FR-13).** A hand-rolled two-language table (EN default, ES), no `intl`/`flutter_localizations`. A global `ValueNotifier<BreezeStrings> breezeStrings` is read and listened to by the tray, overlay, fatal panel, and toasts; `applyUiLanguage(code)` changes it globally and every view re-labels. A pure, case-insensitive `isModelError` helper detects model-integrity errors coming back from Rust so the fatal panel can show reinstall guidance instead of a raw error string.

---

## 4. The FR-13 / i18n approach

Localization is intentionally minimal: two languages, one flat string table per language, one global notifier. This keeps the UI-language switch instantaneous and side-effect-free (no locale reload, no rebuild of the widget tree from an `intl` delegate) and keeps the surface small enough that every user-facing string is visible in one file. Dictation language (what Whisper transcribes) and UI language (the interface) are independent settings, each persisted through its own single-field merge write.

---

## 5. Notable decisions, with the *why*

- **Native-format capture instead of 16 kHz at the device.** WASAPI shared mode serves the mix format; requesting 16 kHz would fail or force exclusive mode. Capturing native and converting at stop time is more robust across devices and keeps the always-on stream cheap.
- **Clipboard restore semantics.** The user's clipboard (text/image/files) is snapshotted before the paste and restored after. Restore failure downgrades the outcome to `PastedRestoreFailed` rather than failing the dictation: the text already landed, which is what the user asked for.
- **`NOACTIVATE` overlay + menu-mode dance.** The overlay must be visible without ever stealing focus, but the tray context menu *needs* foreground activation to behave. So the window carries FR-03 styles by default and drops them only for the moment the menu is up, then puts them back.
- **Single-instance mutex.** One process = one LL hook. Without the guard, a second accidental launch double-installs the hook and every keystroke and paste happens twice. A session-scoped `Local\` mutex (not elevated `Global\`) is the right scope for a per-user app.
- **The no-model state is a hard stop.** If model verification fails, the orchestrator is stopped and the app is paused before any hotkey can arm: a dictation with no model would be a confusing silent failure, so it's surfaced as a fatal, actionable state instead.
