import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

import 'package:inputvoice/src/overlay/overlay_screen.dart';
import 'package:inputvoice/src/rust/api/config.dart' as config_api;
import 'package:inputvoice/src/rust/api/input.dart' as input_api;
import 'package:inputvoice/src/rust/api/instance.dart' as instance_api;
import 'package:inputvoice/src/rust/api/orchestrator.dart' as orchestrator_api;
import 'package:inputvoice/src/rust/api/overlay.dart' as overlay_api;
import 'package:inputvoice/src/rust/api/theme.dart' as theme_api;
import 'package:inputvoice/src/rust/api/transcription.dart' as transcription_api;
import 'package:inputvoice/src/rust/frb_generated.dart';
import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/notifications/notifier.dart';
import 'package:inputvoice/src/tray/tray_controller.dart';

/// Unique Win32 window title. CONTRACT with `rust/src/api/overlay.rs`:
/// Rust locates this window with `FindWindowExW` (runner class +
/// exact title, TD-013) so this literal must stay identical on both
/// sides (FRB cannot share a constant across the boundary, so it
/// travels as an argument).
const String kOverlayWindowTitle = 'Breeze-overlay';

/// FR-03: small frameless pill near the bottom-right corner of the
/// primary work area.
const Size kOverlaySize = Size(180, 48);

/// Margin from the work-area corner.
const double kOverlayMargin = 16;

/// On fatal startup errors the window grows so the message is readable.
const Size kFatalWindowSize = Size(420, 170);

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Single-instance guard (TD-011). Every process installs its own
  // low-level keyboard hook, so a second accidental launch would
  // capture and paste each dictation twice. RustLib.init() must run
  // first because the guard is a Rust (named-mutex) call; it is cheap
  // and installs no hook, engine or audio, so running it before the
  // window setup is safe. If another instance already owns the guard,
  // exit SILENTLY (like most tray apps do) before touching the window,
  // engine, audio or hook. The log line is the only user-visible
  // trace; a transient popup would just add noise.
  await RustLib.init();
  if (!await instance_api.acquireSingleInstance()) {
    debugPrint('Breeze: another instance is already running; exiting.');
    exit(0);
  }

  await windowManager.ensureInitialized();

  // Configure the window BEFORE the runner auto-shows it on the first
  // frame: frameless, tiny, always-on-top, out of the taskbar, with a
  // transparent background (window_manager routes the transparent
  // color through SetWindowCompositionAttribute, giving per-pixel
  // transparency around the rounded pill).
  const options = WindowOptions(
    size: kOverlaySize,
    alwaysOnTop: true,
    skipTaskbar: true,
    titleBarStyle: TitleBarStyle.hidden,
    backgroundColor: Colors.transparent,
    title: kOverlayWindowTitle,
  );
  await windowManager.waitUntilReadyToShow(options);
  await windowManager.setAsFrameless();
  await _placeBottomRight();

  // Error toasts (Task 4). setup() registers the app with the platform
  // notifier; best-effort, a failure only disables toasts. The overlay
  // controller forwards recoverable and fatal error messages to it, and
  // notify() maps + throttles them into the spec flows.
  final notifier = ErrorToastNotifier();
  await notifier.setup();

  final controller = OverlayController(
    windowTitle: kOverlayWindowTitle,
    onErrorMessage: (message) => unawaited(notifier.notify(message)),
  );
  runApp(InputVoiceOverlayApp(controller: controller));

  // Native startup continues after runApp so progress and failures are
  // rendered in the overlay window instead of dying silently.
  unawaited(_startNative(controller, notifier));
}

/// Bottom-right corner of the primary display's work area (excludes
/// the taskbar), inset by [kOverlayMargin]. `setAlignment` computes
/// the work-area corner via screen_retriever inside window_manager.
Future<void> _placeBottomRight() async {
  await windowManager.setAlignment(Alignment.bottomRight);
  final corner = await windowManager.getPosition();
  await windowManager.setPosition(
    Offset(corner.dx - kOverlayMargin, corner.dy - kOverlayMargin),
  );
}

/// Startup sequence (FASE 4 Task 3 + FASE 5 Task 2, FR-09/FR-10):
/// RustLib.init (in main) -> load_config -> init_engine(fullVerify) ->
/// persist model_verified (when fullVerify) -> audio_prewarm ->
/// apply_overlay_styles -> tray init -> subscribe to start_orchestrator
/// (unless config.paused) -> hide (idle starts hidden).
///
/// Any failure before the tray exists is fatal: the overlay shows the
/// error in a readable, enlarged window and the orchestrator is NOT
/// started. With config.paused true the orchestrator is intentionally
/// not started (FR-11): no hook, zero system impact; the tray menu
/// offers "Reanudar detección".
Future<void> _startNative(
  OverlayController controller,
  ErrorToastNotifier notifier,
) async {
  final config_api.AppConfigDto cfg;
  try {
    // RustLib.init() already ran in main() (single-instance guard needs
    // it before any window or engine setup); do NOT call it twice.
    //
    // FR-10 + FR-09: load config FIRST so the first-run marker decides
    // whether this launch does the expensive full SHA-256 verification.
    // loadConfig also applies the persisted dictation language Rust-side.
    cfg = await config_api.loadConfig();
    // FR-13: the interface language drives every visible string from here on.
    applyUiLanguage(cfg.uiLanguage);
    // FR-09: full verification runs only until it has succeeded once
    // (model_verified marker); the resident app then does the fast size
    // check. The overlay says "Verifying model…" only when it applies.
    final fullVerify = !cfg.modelVerified;
    controller.setStarting(
      fullVerify
          ? breezeStrings.value.startupVerifyingModel
          : breezeStrings.value.startupLoadingModel,
    );
    await transcription_api.initEngine(fullVerify: fullVerify);
    // FR-09: record the successful first-run verification so later
    // launches skip it. Best-effort: a persist failure only means the
    // next launch verifies again, which is safe.
    if (fullVerify) {
      try {
        await config_api.updateModelVerified(verified: true);
      } catch (e) {
        debugPrint('Breeze: could not persist model_verified marker: $e');
      }
    }
    controller.setStarting(breezeStrings.value.startupPreparingAudio);
    await input_api.audioPrewarm();
    // From here on the window is click-through and never activates.
    await overlay_api.applyOverlayStyles(windowTitle: kOverlayWindowTitle);
  } catch (e) {
    await _failStartup(controller, e);
    return;
  }

  // Shared with the tray: resuming from pause re-attaches the overlay
  // to a fresh orchestrator stream with the same fatal handling.
  void attachOrchestrator() {
    controller.attach(
      orchestrator_api.startOrchestrator(),
      onFatal: (error) => unawaited(_failStartup(controller, error)),
    );
  }

  // FR-08: the tray lives for the whole process lifetime. A tray
  // failure is logged but NOT fatal - dictation still works without
  // the menu, and killing the app over a cosmetic failure is worse.
  final tray = TrayController(
    overlay: controller,
    initialConfig: cfg,
    attachOrchestrator: attachOrchestrator,
    // FR-09: verify-model failure surfaces through the same fatal path
    // as startup (readable window + no-model tray); success shows a toast.
    onFatal: (error) => unawaited(_failStartup(controller, error)),
    showToast: notifier.showToast,
  );
  try {
    await tray.init();
  } catch (e) {
    debugPrint('Breeze: tray init failed (menu unavailable): $e');
  }

  // FASE 6 (TD-014): pick the tray icon set from the current taskbar
  // theme and swap it live when the user toggles light/dark mode.
  try {
    tray.setTaskbarTheme(await theme_api.getTaskbarLightTheme());
    theme_api.watchTaskbarTheme().listen(
      tray.setTaskbarTheme,
      onError: (Object e) => debugPrint('Breeze theme watcher error: $e'),
    );
  } catch (e) {
    debugPrint('Breeze theme init failed, keeping default icons: $e');
  }

  // FR-11: persisted pause. The orchestrator (and thus the LL hook)
  // is not started at all; the tray shows "Reanudar detección" and
  // resuming goes through attachOrchestrator above.
  if (cfg.paused) {
    controller.setHidden();
    return;
  }

  // Subscribe before hiding so no early state is missed. A stream
  // error here means start_orchestrator itself failed (e.g. the LL
  // hook could not install) - fatal, same path as engine errors.
  attachOrchestrator();

  try {
    // Idle starts hidden (FR-03: the overlay only exists during a
    // dictation cycle); the orchestrator stream wakes it up.
    await overlay_api.hideOverlay(windowTitle: kOverlayWindowTitle);
  } catch (e) {
    await _failStartup(controller, e);
  }
}

/// Fatal startup path: render the error, enlarge the window and show
/// it. `windowManager.show()` (activating) is correct HERE - the app
/// is dead, there is no dictation target to protect; the window must
/// be readable. Caveat: if apply_overlay_styles already ran, the
/// window stays click-through and no-activate, but it remains visible
/// and readable, and the full error is in the log either way.
Future<void> _failStartup(OverlayController controller, Object error) async {
  controller.setFatal(error.toString());
  try {
    await windowManager.setSize(kFatalWindowSize);
    await _placeBottomRight();
    await windowManager.show();
  } catch (e) {
    debugPrint('Breeze: could not surface the fatal window: $e');
  }
}

class InputVoiceOverlayApp extends StatelessWidget {
  const InputVoiceOverlayApp({super.key, required this.controller});

  final OverlayController controller;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Breeze',
      debugShowCheckedModeBanner: false,
      color: Colors.transparent,
      theme: ThemeData(brightness: Brightness.dark),
      home: Scaffold(
        backgroundColor: Colors.transparent,
        body: OverlayScreen(controller: controller),
      ),
    );
  }
}
