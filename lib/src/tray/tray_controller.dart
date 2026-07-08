import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:tray_manager/tray_manager.dart';

import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/overlay/overlay_screen.dart';
import 'package:inputvoice/src/rust/api/config.dart' as config_api;
import 'package:inputvoice/src/rust/api/input.dart' as input_api;
import 'package:inputvoice/src/rust/api/orchestrator.dart' as orchestrator_api;
import 'package:inputvoice/src/rust/api/overlay.dart' as overlay_api;
import 'package:inputvoice/src/rust/api/transcription.dart' as transcription_api;

/// Stable menu item keys for click dispatch.
const String kTrayKeyTogglePause = 'toggle-pause';
const String kTrayKeyLangEs = 'lang-es';
const String kTrayKeyLangEn = 'lang-en';
const String kTrayKeyUiLangEs = 'ui-lang-es';
const String kTrayKeyUiLangEn = 'ui-lang-en';
const String kTrayKeyOpenLogs = 'open-logs';
const String kTrayKeyVerifyModel = 'verify-model';
const String kTrayKeyQuit = 'quit';

/// Short user-facing state label shown in the tray tooltip and in the
/// disabled status line of the menu. Pure function so the mapping is
/// unit-testable. `paused` wins over the overlay phase because a paused
/// app has no orchestrator stream and its last phase is stale.
String trayStateLabel(
  OverlayPhase phase, {
  required bool paused,
  required BreezeStrings strings,
}) {
  if (paused) {
    return strings.statePaused;
  }
  if (phase is PhaseStarting) {
    return strings.overlayStarting;
  }
  if (phase is PhaseListening) {
    return strings.overlayListening;
  }
  if (phase is PhaseTranscribing) {
    return strings.overlayTranscribing;
  }
  if (phase is PhaseInjecting) {
    return strings.overlayPasting;
  }
  if (phase is PhaseError) {
    return _shortError(phase.message, strings.errorPrefix);
  }
  if (phase is PhaseFatal) {
    return strings.stateFatal;
  }
  return strings.stateActive;
}

/// Tooltip-sized error (docs, 04-ux-requirements asks for short error
/// surfaces; the full message is always in the log).
String _shortError(String message, String prefix) {
  final trimmed = message.trim();
  if (trimmed.length <= 40) {
    return '$prefix$trimmed';
  }
  return '$prefix${trimmed.substring(0, 40)}…';
}

/// Tray icon asset for the current state and taskbar theme (TD-014,
/// FASE 6). Dark taskbar (lightTaskbar == false) gets the light-logo
/// variants so the icon stays visible; light taskbar gets the dark
/// logo. Pure function so the mapping is unit-testable.
String trayIconAsset(
  OverlayPhase phase, {
  required bool paused,
  required bool lightTaskbar,
}) {
  final suffix = lightTaskbar ? '' : '_light';
  // FR-09: the fatal / no-model state reuses the paused (gray) icon so
  // it is visually distinct from idle/recording without a new asset.
  if (paused || phase is PhaseFatal) {
    return 'assets/breeze_paused$suffix.ico';
  }
  final recording = phase is PhaseListening ||
      phase is PhaseTranscribing ||
      phase is PhaseInjecting;
  return recording
      ? 'assets/breeze_recording$suffix.ico'
      : 'assets/breeze_idle$suffix.ico';
}

/// System tray for Breeze (FR-08): resident icon, pause and resume of
/// the hotkey detection (FR-11, early), ES-EN language selector
/// (FR-07), a logs shortcut and clean quit.
///
/// State sharing decision: the tray does NOT subscribe to the Rust
/// orchestrator stream a second time. [OverlayController] is already a
/// ValueNotifier mirroring that stream, so the tray listens to it and
/// derives the tooltip and the status-line text. One stream, two views.
///
/// Icon design note (TD-014): the tray icon now reflects the runtime
/// state with three variants derived from the Breeze logo - idle (the
/// plain logo), recording (a malva/red status dot) and paused (a dimmed
/// grayscale logo). The state also travels in the tooltip and the menu
/// title. Error keeps the idle icon (the tooltip carries the message).
class TrayController with TrayListener {
  TrayController({
    required this.overlay,
    required config_api.AppConfigDto initialConfig,
    required this.attachOrchestrator,
    required this.onFatal,
    required this.showToast,
  }) : _config = initialConfig,
       _paused = initialConfig.paused;

  /// Shared cycle-state source (see class docs).
  final OverlayController overlay;

  /// Re-attaches the overlay to a fresh `start_orchestrator` stream on
  /// resume. Owned by main.dart, which also owns fatal-error handling.
  final void Function() attachOrchestrator;

  /// FR-09: surfaces a fatal error the same way startup does (enlarged,
  /// readable window + no-model tray). Owned by main.dart.
  final void Function(Object error) onFatal;

  /// FR-09: shows a system toast (title, body). Wired by main.dart to
  /// the ErrorToastNotifier so the verify-OK confirmation reaches the OS.
  final Future<void> Function(String title, String body) showToast;

  config_api.AppConfigDto _config;
  bool _paused;
  bool _lightTaskbar = false;
  String _lastLabel = '';
  String _lastIcon = '';

  /// Whether hotkey detection is currently paused (FR-11).
  bool get paused => _paused;

  /// Creates the tray icon, tooltip and initial menu, then starts
  /// mirroring the overlay phase into the tooltip and menu title.
  Future<void> init() async {
    trayManager.addListener(this);
    await _applyIcon();
    overlay.addListener(_onOverlayPhase);
    breezeStrings.addListener(_onStringsChanged);
    await _refresh();
    debugPrint(
      'Breeze tray: initialized (paused=$_paused, language=${_config.language})',
    );
  }

  /// Detaches listeners and removes the tray icon.
  Future<void> dispose() async {
    breezeStrings.removeListener(_onStringsChanged);
    overlay.removeListener(_onOverlayPhase);
    trayManager.removeListener(this);
    await trayManager.destroy();
  }

  void _onOverlayPhase() {
    final label = trayStateLabel(
      overlay.value,
      paused: _paused,
      strings: breezeStrings.value,
    );
    if (label == _lastLabel) {
      return; // avoid churning the menu on repeated equal states
    }
    unawaited(_refresh());
  }

/// FR-13: a UI language change re-labels the tooltip and the menu.
  void _onStringsChanged() {
    unawaited(_refresh());
  }

  /// Rebuilds tooltip and menu from the current state. Rebuilding the
  /// whole menu keeps every label (status line, pause toggle, language
  /// checks) consistent through a single code path.
  Future<void> _refresh() async {
    final strings = breezeStrings.value;
    final label = trayStateLabel(
      overlay.value,
      paused: _paused,
      strings: strings,
    );
    _lastLabel = label;
    String tooltip;
    if (label == strings.stateActive) {
      tooltip = strings.trayTooltipIdle;
    } else {
      tooltip = 'Breeze — $label';
    }
    try {
      await _applyIcon();
      await trayManager.setToolTip(tooltip);
      await trayManager.setContextMenu(_buildMenu(label, strings));
    } catch (e) {
      debugPrint('Breeze tray: refresh failed: $e');
    }
  }

  /// FASE 6: swaps the icon set when the Windows taskbar theme changes.
  /// Called with the initial theme at startup and by the Rust
  /// WM_SETTINGCHANGE watcher afterwards.
  void setTaskbarTheme(bool light) {
    if (light == _lightTaskbar) {
      return;
    }
    _lightTaskbar = light;
    _lastIcon = ''; // force the native call on the next apply
    unawaited(_applyIcon());
  }

  /// Selects the tray icon from the current state and taskbar theme
  /// (TD-014, FASE 6). Delegates the mapping to the pure [trayIconAsset]
  /// and skips the native call when the icon would not change.
  Future<void> _applyIcon() async {
    final icon = trayIconAsset(
      overlay.value,
      paused: _paused,
      lightTaskbar: _lightTaskbar,
    );
    if (icon == _lastIcon) {
      return;
    }
    _lastIcon = icon;
    await trayManager.setIcon(icon);
  }

  Menu _buildMenu(String stateLabel, BreezeStrings strings) {
    String pauseLabel;
    if (_paused) {
      pauseLabel = strings.menuResume;
    } else {
      pauseLabel = strings.menuPause;
    }
    return Menu(
      items: [
        MenuItem(label: 'Breeze — $stateLabel', disabled: true),
        MenuItem.separator(),
        MenuItem(key: kTrayKeyTogglePause, label: pauseLabel),
        MenuItem.separator(),
        MenuItem.submenu(
          label: strings.menuDictationLanguage,
          submenu: Menu(
            items: [
              MenuItem.checkbox(
                key: kTrayKeyLangEs,
                label: strings.languageSpanish,
                checked: _config.language == 'es',
              ),
              MenuItem.checkbox(
                key: kTrayKeyLangEn,
                label: strings.languageEnglish,
                checked: _config.language == 'en',
              ),
            ],
          ),
        ),
        MenuItem.submenu(
          label: strings.menuUiLanguage,
          submenu: Menu(
            items: [
              MenuItem.checkbox(
                key: kTrayKeyUiLangEs,
                label: strings.languageSpanish,
                checked: _config.uiLanguage == 'es',
              ),
              MenuItem.checkbox(
                key: kTrayKeyUiLangEn,
                label: strings.languageEnglish,
                checked: _config.uiLanguage == 'en',
              ),
            ],
          ),
        ),
        MenuItem.separator(),
        MenuItem(key: kTrayKeyOpenLogs, label: strings.menuOpenLogs),
        MenuItem(key: kTrayKeyVerifyModel, label: strings.menuVerifyModel),
        MenuItem.separator(),
        MenuItem(key: kTrayKeyQuit, label: strings.menuQuit),
      ],
    );
  }

  // NOTE: tray_manager on Windows dispatches onTrayIcon(Right)MouseDown on
  // WM_(L/R)BUTTONUP - the ...MouseUp overrides never fire here. Verified
  // against tray_manager 0.5.3 tray_manager_plugin.cpp.
  @override
  void onTrayIconMouseDown() {
    unawaited(_popMenu());
  }

  @override
  void onTrayIconRightMouseDown() {
    unawaited(_popMenu());
  }

  /// Pops the tray menu. The overlay window carries WS_EX_NOACTIVATE, which
  /// blocks the SetForegroundWindow the tray plugin needs, so we briefly put
  /// the window in menu-capable mode around the (modal) popup and restore the
  /// FR-03 styles afterwards, re-hiding only if the overlay was idle.
  Future<void> _popMenu() async {
    final wasHidden = overlay.value is PhaseHidden;
    try {
      await overlay_api.enableMenuMode(windowTitle: overlay.windowTitle);
      await trayManager.popUpContextMenu();
    } finally {
      await overlay_api.disableMenuMode(
        windowTitle: overlay.windowTitle,
        hide_: wasHidden,
      );
    }
  }

  @override
  void onTrayMenuItemClick(MenuItem menuItem) {
    final key = menuItem.key;
    if (key == kTrayKeyTogglePause) {
      unawaited(_togglePause());
    } else if (key == kTrayKeyLangEs) {
      unawaited(_setLanguage('es'));
    } else if (key == kTrayKeyLangEn) {
      unawaited(_setLanguage('en'));
    } else if (key == kTrayKeyUiLangEs) {
      unawaited(_setUiLanguage('es'));
    } else if (key == kTrayKeyUiLangEn) {
      unawaited(_setUiLanguage('en'));
    } else if (key == kTrayKeyOpenLogs) {
      unawaited(_openLogsFolder());
    } else if (key == kTrayKeyVerifyModel) {
      unawaited(_verifyModel());
    } else if (key == kTrayKeyQuit) {
      unawaited(_quit());
    }
  }

  /// FR-11 pause: stop_orchestrator uninstalls the LL hook (the
  /// monitor logs HookUninstalled, zero system impact) and stops the
  /// runtime; the whisper engine and the prewarmed audio capture stay
  /// alive process-wide, so resume is instant. `paused` is persisted
  /// so a restart honors it (main.dart skips the orchestrator).
  Future<void> _togglePause() async {
    if (_paused) {
      attachOrchestrator();
      _paused = false;
    } else {
      try {
        await orchestrator_api.stopOrchestrator();
      } catch (e) {
        debugPrint('Breeze tray: stop_orchestrator failed: $e');
      }
      _paused = true;
      overlay.setHidden();
    }
    // I-1: merge-style persist. The tray must never write its (possibly
    // stale) _config snapshot wholesale, because that would revert
    // model_verified written by main.dart or a failed on-demand verify
    // and invert FR-09 semantics. update_paused mutates only `paused`
    // Rust-side. The local snapshot is kept in sync for display only.
    _persistPausedSnapshot(_paused);
    await _persistPaused(_paused);
    await _refresh();
  }

  /// FR-07: apply and persist the language. update_language validates
  /// the value, persists config.json and applies the transcription
  /// global in Rust; the next dictation already uses it.
  Future<void> _setLanguage(String lang) async {
    if (lang != _config.language) {
      try {
        await config_api.updateLanguage(lang: lang);
        _config = config_api.AppConfigDto(
          language: lang,
          paused: _config.paused,
          logLevel: _config.logLevel,
          uiLanguage: _config.uiLanguage,
          modelVerified: _config.modelVerified,
        );
      } catch (e) {
        debugPrint('Breeze tray: language update failed: $e');
      }
    }
    await _refresh();
  }

/// FR-13: apply and persist the interface language. Rust validates
/// and persists ui_language without touching the dictation language;
/// the global notifier then re-labels the overlay, the fatal panel,
/// the toasts and this menu.
  Future<void> _setUiLanguage(String lang) async {
    if (lang != _config.uiLanguage) {
      try {
        await config_api.updateUiLanguage(lang: lang);
        _config = config_api.AppConfigDto(
          language: _config.language,
          paused: _config.paused,
          logLevel: _config.logLevel,
          uiLanguage: lang,
          modelVerified: _config.modelVerified,
        );
        applyUiLanguage(lang);
      } catch (e) {
        debugPrint('Breeze tray: ui language update failed: $e');
      }
    }
    await _refresh();
  }

  /// Keeps the local [_config] snapshot's `paused` in sync for menu/tooltip
  /// display only. This snapshot is NEVER written to disk wholesale anymore
  /// (I-1) — [_persistPaused] does the merge-style Rust write instead.
  void _persistPausedSnapshot(bool paused) {
    _config = config_api.AppConfigDto(
      language: _config.language,
      paused: paused,
      logLevel: _config.logLevel,
      uiLanguage: _config.uiLanguage,
      modelVerified: _config.modelVerified,
    );
  }

  /// I-1: persists only the `paused` flag through the merge-style Rust API
  /// (update_paused loads current config, mutates paused, saves atomically),
  /// so a pause toggle can never clobber model_verified or another field.
  Future<void> _persistPaused(bool paused) async {
    try {
      await config_api.updatePaused(paused: paused);
    } catch (e) {
      debugPrint('Breeze tray: paused persist failed: $e');
    }
  }

  /// FR-09 (TD-004): on-demand SHA-256 verification from the tray. Runs
  /// the Rust verify_model (locate + hash, no engine reload). On success
  /// a confirmation toast; on failure the marker is reset so the next
  /// launch re-verifies, the overlay shows the reinstall guidance and the
  /// tray drops to the no-model (paused/gray) icon via onFatal.
  Future<void> _verifyModel() async {
    final strings = breezeStrings.value;
    try {
      await transcription_api.verifyModel();
      await showToast(strings.toastModelOkTitle, strings.toastModelOkBody);
    } catch (e) {
      debugPrint('Breeze tray: model verification failed: $e');
      // I-2 / FR-09: a failed on-demand verify must leave NO active hotkey
      // in the no-model state. Previously the orchestrator kept running
      // (hook installed, dictation functional but invisible) behind the
      // fatal panel. So, in order: (1) stop the orchestrator so the LL hook
      // is uninstalled, (2) latch paused and persist it via the merge-style
      // update_paused so a restart also comes up paused, (3) reset the
      // model_verified marker so the next launch re-verifies, (4) surface
      // the fatal overlay + no-model tray, (5) rebuild the menu/icon.
      try {
        await orchestrator_api.stopOrchestrator();
      } catch (stopError) {
        debugPrint('Breeze tray: stop after verify failure failed: $stopError');
      }
      _paused = true;
      _persistPausedSnapshot(true);
      await _persistPaused(true);
      try {
        await config_api.updateModelVerified(verified: false);
      } catch (persistError) {
        debugPrint('Breeze tray: reset model_verified failed: $persistError');
      }
      onFatal(e);
      await _refresh();
    }
  }

  /// Opens the Breeze log folder in Explorer.
  Future<void> _openLogsFolder() async {
    final appData = Platform.environment['APPDATA'];
    if (appData == null || appData.isEmpty) {
      debugPrint('Breeze tray: APPDATA unresolved, cannot open logs');
      return;
    }
    final dir = '$appData\\Breeze\\logs';
    try {
      await Process.start('explorer', [dir], mode: ProcessStartMode.detached);
    } catch (e) {
      debugPrint('Breeze tray: could not open the logs folder: $e');
    }
  }

  /// Clean shutdown: stop the orchestrator (hook uninstalled), remove
  /// the tray icon and end the process.
  Future<void> _quit() async {
    if (!_paused) {
      try {
        await orchestrator_api.stopOrchestrator();
      } catch (e) {
        debugPrint('Breeze tray: stop during quit failed: $e');
      }
    }
    try {
      await input_api.clearLastTranscription();
    } catch (e) {
      debugPrint('Breeze tray: clear_last_transcription failed: $e');
    }
    await trayManager.destroy();
    exit(0);
  }
}
