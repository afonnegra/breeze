import 'package:flutter/foundation.dart';

/// User-facing UI strings for Breeze (FR-13): English and Spanish.
/// Hand-rolled table (no flutter_localizations, no intl) because the
/// app ships exactly two languages and about two dozen strings.
/// Overlay state labels start with a capital letter in BOTH languages
/// (explicit FR-13 requirement).
class BreezeStrings {
  const BreezeStrings({
    required this.languageCode,
    required this.overlayStarting,
    required this.overlayListening,
    required this.overlayTranscribing,
    required this.overlayPasting,
    required this.startupLoadingModel,
    required this.startupVerifyingModel,
    required this.startupPreparingAudio,
    required this.fatalTitle,
    required this.fatalModelBody,
    required this.fatalLogsHint,
    required this.trayTooltipIdle,
    required this.stateActive,
    required this.statePaused,
    required this.stateFatal,
    required this.errorPrefix,
    required this.menuPause,
    required this.menuResume,
    required this.menuDictationLanguage,
    required this.menuUiLanguage,
    required this.menuOpenLogs,
    required this.menuVerifyModel,
    required this.menuQuit,
    required this.languageSpanish,
    required this.languageEnglish,
    required this.toastGpuTitle,
    required this.toastGpuBody,
    required this.toastMicTitle,
    required this.toastMicBody,
    required this.toastModelOkTitle,
    required this.toastModelOkBody,
  });

  final String languageCode;
  final String overlayStarting;
  final String overlayListening;
  final String overlayTranscribing;
  final String overlayPasting;
  final String startupLoadingModel;
  final String startupVerifyingModel;
  final String startupPreparingAudio;
  final String fatalTitle;
  final String fatalModelBody;
  final String fatalLogsHint;
  final String trayTooltipIdle;
  final String stateActive;
  final String statePaused;
  final String stateFatal;
  final String errorPrefix;
  final String menuPause;
  final String menuResume;
  final String menuDictationLanguage;
  final String menuUiLanguage;
  final String menuOpenLogs;
  final String menuVerifyModel;
  final String menuQuit;
  final String languageSpanish;
  final String languageEnglish;
  final String toastGpuTitle;
  final String toastGpuBody;
  final String toastMicTitle;
  final String toastMicBody;
  final String toastModelOkTitle;
  final String toastModelOkBody;

  static const BreezeStrings en = BreezeStrings(
    languageCode: 'en',
    overlayStarting: 'Starting…',
    overlayListening: 'Listening…',
    overlayTranscribing: 'Transcribing…',
    overlayPasting: 'Pasting…',
    startupLoadingModel: 'Loading model…',
    startupVerifyingModel: 'Verifying model…',
    startupPreparingAudio: 'Preparing audio…',
    fatalTitle: 'Breeze could not start',
    fatalModelBody:
        'The voice model is missing or corrupted. Reinstall Breeze to '
        'restore it.',
    fatalLogsHint:
        'Check the logs in %APPDATA%\\Breeze\\logs and close this process.',
    trayTooltipIdle: 'Breeze — voice dictation',
    stateActive: 'Active',
    statePaused: 'Paused',
    stateFatal: 'Fatal error',
    errorPrefix: 'Error: ',
    menuPause: 'Pause detection',
    menuResume: 'Resume detection',
    menuDictationLanguage: 'Dictation language',
    menuUiLanguage: 'Interface language',
    menuOpenLogs: 'Open logs folder',
    menuVerifyModel: 'Verify model',
    menuQuit: 'Quit',
    languageSpanish: 'Español',
    languageEnglish: 'English',
    toastGpuTitle: 'Breeze: GPU out of memory',
    toastGpuBody:
        'The GPU has no memory available. Close the applications using '
        'the graphics card and try again.',
    toastMicTitle: 'Breeze: microphone unavailable',
    toastMicBody:
        'The microphone cannot be accessed. Check that it is not in use '
        'by another application.',
    toastModelOkTitle: 'Breeze: model verified OK',
    toastModelOkBody: 'The voice model passed the integrity check.',
  );

  static const BreezeStrings es = BreezeStrings(
    languageCode: 'es',
    overlayStarting: 'Iniciando…',
    overlayListening: 'Escuchando…',
    overlayTranscribing: 'Transcribiendo…',
    overlayPasting: 'Pegando…',
    startupLoadingModel: 'Cargando modelo…',
    startupVerifyingModel: 'Verificando modelo…',
    startupPreparingAudio: 'Preparando audio…',
    fatalTitle: 'Breeze no pudo iniciar',
    fatalModelBody:
        'El modelo de voz falta o está dañado. Reinstala Breeze para '
        'restaurarlo.',
    fatalLogsHint:
        'Revisa los logs en %APPDATA%\\Breeze\\logs y cierra este proceso.',
    trayTooltipIdle: 'Breeze — dictado por voz',
    stateActive: 'Activo',
    statePaused: 'Pausado',
    stateFatal: 'Error fatal',
    errorPrefix: 'Error: ',
    menuPause: 'Pausar detección',
    menuResume: 'Reanudar detección',
    menuDictationLanguage: 'Idioma de dictado',
    menuUiLanguage: 'Idioma de interfaz',
    menuOpenLogs: 'Abrir carpeta de logs',
    menuVerifyModel: 'Verificar modelo',
    menuQuit: 'Salir',
    languageSpanish: 'Español',
    languageEnglish: 'English',
    toastGpuTitle: 'Breeze: GPU sin memoria',
    toastGpuBody:
        'La GPU no tiene memoria disponible. Cierra las aplicaciones que '
        'usen la tarjeta gráfica e inténtalo de nuevo.',
    toastMicTitle: 'Breeze: micrófono no disponible',
    toastMicBody:
        'No se puede acceder al micrófono. Revisa que no esté en '
        'uso por otra aplicación.',
    toastModelOkTitle: 'Breeze: modelo verificado OK',
    toastModelOkBody: 'El modelo de voz pasó la verificación de integridad.',
  );

  /// Table for [code]; anything unsupported falls back to English,
  /// the FR-13 default.
  static BreezeStrings forLanguage(String code) {
    if (code == 'es') {
      return es;
    }
    return en;
  }
}

/// Single source of truth for the active UI language. Tray, overlay,
/// fatal panel and toasts all read (and listen to) this notifier, so a
/// change made from the tray propagates everywhere at once (FR-13).
final ValueNotifier<BreezeStrings> breezeStrings =
    ValueNotifier<BreezeStrings>(BreezeStrings.en);

/// Applies [code] to the global notifier ("en" when unsupported).
void applyUiLanguage(String code) {
  breezeStrings.value = BreezeStrings.forLanguage(code);
}

/// FR-09: whether a fatal/verify error message denotes a model problem
/// (missing, wrong size or bad hash), so the fatal panel and the tray
/// can show the reinstall guidance instead of the raw error. Pure and
/// case-insensitive, matching substrings of the literals Rust emits
/// (TranscriptionError.modelMissing() / .modelCorrupt(), and the
/// underlying "model ..." messages), mirroring notifier.dart's approach.
bool isModelError(String message) {
  final m = message.toLowerCase();
  return m.contains('modelmissing') ||
      m.contains('modelcorrupt') ||
      m.contains('model not found') ||
      m.contains('integrity check') ||
      m.contains('size or hash') ||
      m.contains('hash mismatch') ||
      m.contains('size mismatch');
}
