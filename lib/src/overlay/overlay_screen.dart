import 'dart:async';

import 'package:flutter/material.dart';

import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/rust/api/orchestrator.dart';
import 'package:inputvoice/src/rust/api/overlay.dart' as overlay_api;

// ---------------------------------------------------------------------------
// Breeze visual identity (TD-014). Palette extracted from the official logo
// (assets/breeze_logo.png, 2000px source): a rounded purple-violet gradient shape.
// ---------------------------------------------------------------------------

/// Deep indigo: top of the overlay gradient and shadow base.
const Color kBreezeIndigo = Color(0xFF3A2167);

/// Brand purple: mid-tone used for the icon halo and highlights.
const Color kBreezePurple = Color(0xFF5B3366);

/// Magenta-rose: bottom of the real logo gradient (indigo -> magenta,
/// vertical), the sphere in assets/breeze_logo.png.
const Color kBreezeMagenta = Color(0xFFB5476A);

/// Warm malva accent: the recording dot and highlights.
const Color kBreezeMalva = Color(0xFF724364);

/// Reddish error tint, kept inside the purple container.
const Color kBreezeError = Color(0xFFB5476A);

/// Primary text on purple.
const Color kBreezeText = Color(0xFFFFFFFF);

/// Solid dark background for the overlay (user request): black #333333,
/// with the brand gradient used only as an accent border.
const Color kBreezeBg = Color(0xFF333333);

/// Secondary text on purple (white at ~70%).
const Color kBreezeTextDim = Color(0xB3FFFFFF);

/// Visual phase of the overlay (FR-03 + docs/04-ux-requirements.md).
///
/// Mirrors the Rust `UiStateDto` stream plus two Dart-only startup
/// phases: [PhaseStarting] (engine/model loading) and [PhaseFatal]
/// (startup failed, the app cannot dictate).
sealed class OverlayPhase {
  const OverlayPhase();
}

/// Startup in progress (RustLib / whisper model / audio prewarm).
class PhaseStarting extends OverlayPhase {
  const PhaseStarting(this.message);

  final String message;
}

/// Idle: no cycle in flight, the native window is hidden.
class PhaseHidden extends OverlayPhase {
  const PhaseHidden();
}

/// Recording (combo held).
class PhaseListening extends OverlayPhase {
  const PhaseListening();
}

/// Whisper is processing the captured audio.
class PhaseTranscribing extends OverlayPhase {
  const PhaseTranscribing();
}

/// Text is being pasted into the focused control.
class PhaseInjecting extends OverlayPhase {
  const PhaseInjecting();
}

/// A dictation cycle failed (recoverable). Short message, truncated.
class PhaseError extends OverlayPhase {
  const PhaseError(this.message);

  final String message;
}

/// Startup failed: the app cannot work. main.dart enlarges and shows
/// the window so the user can read the reason and close the process.
class PhaseFatal extends OverlayPhase {
  const PhaseFatal(this.message);

  final String message;
}

/// Bridges the Rust orchestrator `UiStateDto` stream to the overlay UI
/// and drives the native window visibility through the Rust overlay
/// api (`SW_SHOWNOACTIVATE` / `SW_HIDE`). Never uses
/// `windowManager.show()`, which would activate the window and steal
/// focus from the dictation target (FR-03.AC-3).
class OverlayController extends ValueNotifier<OverlayPhase> {
  OverlayController({required this.windowTitle, this.onErrorMessage})
    : super(PhaseStarting(breezeStrings.value.overlayStarting));

  /// Win32 title of the overlay window - the exact-match contract with
  /// `rust/src/api/overlay.rs` (see `kOverlayWindowTitle` in main.dart).
  final String windowTitle;

  /// Optional sink for user-facing error messages: recoverable cycle
  /// errors (Error state) and fatal startup errors. main.dart wires it
  /// to the ErrorToastNotifier so the mapped flows raise a system toast.
  final void Function(String message)? onErrorMessage;

  /// Anti-flicker (docs/04-ux-requirements.md): on `Hidden` the last
  /// visual persists up to 300 ms before the window hides, so
  /// back-to-back cycles do not blink. Cancelled by any other state.
  static const Duration hidePersistence = Duration(milliseconds: 300);

  StreamSubscription<UiStateDto>? _sub;
  Timer? _hideTimer;

  void setStarting(String message) {
    value = PhaseStarting(message);
  }

  void setFatal(String message) {
    _hideTimer?.cancel();
    _hideTimer = null;
    value = PhaseFatal(message);
    onErrorMessage?.call(message);
  }

  /// Forces the hidden state and hides the native window. Used by the
  /// tray pause (FR-11): once the orchestrator stops, no further state
  /// arrives, so the overlay must not stay frozen on screen.
  void setHidden() {
    _hideTimer?.cancel();
    _hideTimer = null;
    value = const PhaseHidden();
    unawaited(_hideWindow());
  }

  /// Subscribes to the orchestrator state stream. A stream error means
  /// the orchestrator is gone (e.g. `start_orchestrator` failed), which
  /// is fatal for a dictation app - main.dart handles it via [onFatal].
  void attach(
    Stream<UiStateDto> stream, {
    required void Function(Object error) onFatal,
  }) {
    unawaited(_sub?.cancel()); // re-attach after pause (FR-11)
    _sub = stream.listen(_onUiState, onError: onFatal);
  }

  void _onUiState(UiStateDto state) {
    if (value is PhaseFatal) {
      return; // a dead app has no live overlay states to render
    }
    if (state is UiStateDto_Hidden) {
      _hideTimer?.cancel();
      _hideTimer = Timer(hidePersistence, () {
        value = const PhaseHidden();
        unawaited(_hideWindow());
      });
      return;
    }
    _hideTimer?.cancel();
    _hideTimer = null;
    // Show first (no activation), then render the phase.
    unawaited(_showWindow());
    value = switch (state) {
      UiStateDto_Listening() => const PhaseListening(),
      UiStateDto_Transcribing() => const PhaseTranscribing(),
      UiStateDto_Injecting() => const PhaseInjecting(),
      UiStateDto_Error(:final message) => PhaseError(message),
      // Unreachable: handled above; keeps the switch exhaustive.
      UiStateDto_Hidden() => const PhaseHidden(),
    };
    if (state is UiStateDto_Error) {
      // Recoverable cycle error - let main.dart decide whether it maps
      // to a toast (throttled). The overlay already shows the message.
      onErrorMessage?.call(state.message);
    }
  }

  Future<void> _showWindow() async {
    try {
      await overlay_api.showOverlayNoActivate(windowTitle: windowTitle);
    } catch (e) {
      debugPrint('Breeze overlay: show failed: $e');
    }
  }

  Future<void> _hideWindow() async {
    try {
      await overlay_api.hideOverlay(windowTitle: windowTitle);
    } catch (e) {
      debugPrint('Breeze overlay: hide failed: $e');
    }
  }

  @override
  void dispose() {
    _hideTimer?.cancel();
    unawaited(_sub?.cancel());
    super.dispose();
  }
}

/// The overlay itself: a compact rounded pill with the Breeze purple
/// gradient, state-driven (FR-03 visual states).
class OverlayScreen extends StatelessWidget {
  const OverlayScreen({super.key, required this.controller});

  final OverlayController controller;

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<BreezeStrings>(
      valueListenable: breezeStrings,
      builder: (context, strings, _) => ValueListenableBuilder<OverlayPhase>(
        valueListenable: controller,
        builder: (context, phase, _) => switch (phase) {
          PhaseHidden() => const SizedBox.shrink(),
          PhaseStarting(:final message) => _Pill(
            icon: const _SmallSpinner(),
            text: message,
          ),
          PhaseListening() => _Pill(
            icon: const _PulsingDot(),
            text: strings.overlayListening,
          ),
          PhaseTranscribing() => _Pill(
            icon: const _SmallSpinner(),
            text: strings.overlayTranscribing,
          ),
          PhaseInjecting() => _Pill(
            icon: const Icon(
              Icons.check_rounded,
              size: 16,
              color: kBreezeText,
            ),
            text: strings.overlayPasting,
          ),
          PhaseError(:final message) => _Pill(
            icon: const Icon(
              Icons.warning_amber_rounded,
              size: 16,
              color: kBreezeError,
            ),
            text: message,
            tint: kBreezeError,
          ),
          PhaseFatal(:final message) =>
              _FatalPanel(message: message, strings: strings),
        },
      ),
    );
  }
}

/// The Breeze pill: a compact rounded container with a diagonal purple
/// gradient, a soft shadow, a subtle 1px white border for definition on
/// light backgrounds, an icon and one short line of medium-weight text.
class _Pill extends StatelessWidget {
  const _Pill({required this.icon, required this.text, this.tint});

  final Widget icon;
  final String text;

  /// Optional accent used to tint the icon halo (error state). The
  /// container itself stays purple so the identity is never lost.
  final Color? tint;

  @override
  Widget build(BuildContext context) {
    final haloColor = (tint ?? kBreezeMalva).withValues(alpha: 0.28);
    return Center(
      child: Container(
        constraints: const BoxConstraints(minWidth: 120, maxWidth: 176),
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
        decoration: BoxDecoration(
          color: kBreezeBg,
          borderRadius: BorderRadius.circular(22),
          // Brand color lives only in the border (user request): black fill,
          // magenta accent border.
          border: Border.all(color: kBreezeMagenta, width: 2),
          boxShadow: [
            BoxShadow(
              color: kBreezeMagenta.withValues(alpha: 0.30),
              blurRadius: 16,
              spreadRadius: 0,
              offset: const Offset(0, 5),
            ),
          ],
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            // A soft round halo behind the status icon keeps the small
            // glyphs legible over the gradient.
            Container(
              width: 22,
              height: 22,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: haloColor,
                shape: BoxShape.circle,
              ),
              child: icon,
            ),
            const SizedBox(width: 10),
            Flexible(
              child: Text(
                text,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: kBreezeText,
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.2,
                  decoration: TextDecoration.none,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Fatal startup panel with the Breeze identity: purple gradient, the
/// brand mark on top, the full message in white and a readable log hint.
/// The window was enlarged and shown by main.dart; the user closes it.
class _FatalPanel extends StatelessWidget {
  const _FatalPanel({required this.message, required this.strings});

  final String message;
  final BreezeStrings strings;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(18),
      decoration: BoxDecoration(
        color: kBreezeBg,
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: kBreezeMagenta, width: 2),
        boxShadow: [
          BoxShadow(
            color: kBreezeMagenta.withValues(alpha: 0.35),
            blurRadius: 22,
            offset: const Offset(0, 8),
          ),
        ],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 26,
                height: 26,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: kBreezeError.withValues(alpha: 0.3),
                  shape: BoxShape.circle,
                ),
                child: const Icon(
                  Icons.error_outline_rounded,
                  size: 17,
                  color: kBreezeText,
                ),
              ),
              const SizedBox(width: 10),
              Text(
                strings.fatalTitle,
                style: const TextStyle(
                  color: kBreezeText,
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                  letterSpacing: 0.2,
                  decoration: TextDecoration.none,
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Expanded(
            child: SingleChildScrollView(
              // FR-09: a model error (missing / corrupt) shows the
              // reinstall guidance instead of the raw Rust error text;
              // any other fatal keeps the exact message for diagnosis.
              child: Text(
                isModelError(message) ? strings.fatalModelBody : message,
                style: const TextStyle(
                  color: kBreezeTextDim,
                  fontSize: 12,
                  height: 1.35,
                  decoration: TextDecoration.none,
                ),
              ),
            ),
          ),
          const SizedBox(height: 10),
          Text(
            strings.fatalLogsHint,
            style: const TextStyle(
              color: kBreezeTextDim,
              fontSize: 11,
              decoration: TextDecoration.none,
            ),
          ),
        ],
      ),
    );
  }
}

/// Thin white circular spinner (transcribing / starting).
class _SmallSpinner extends StatelessWidget {
  const _SmallSpinner();

  @override
  Widget build(BuildContext context) {
    return const SizedBox(
      width: 13,
      height: 13,
      child: CircularProgressIndicator(
        strokeWidth: 2,
        valueColor: AlwaysStoppedAnimation<Color>(kBreezeText),
      ),
    );
  }
}

/// Breathing malva dot for the listening state (FR-03: animated
/// recording dot). Scales and fades together on a soft ~1s cycle.
class _PulsingDot extends StatefulWidget {
  const _PulsingDot();

  @override
  State<_PulsingDot> createState() => _PulsingDotState();
}

class _PulsingDotState extends State<_PulsingDot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse;

  @override
  void initState() {
    super.initState();
    _pulse = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1000),
    )..repeat(reverse: true);
  }

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final curved = CurvedAnimation(parent: _pulse, curve: Curves.easeInOut);
    return FadeTransition(
      opacity: Tween<double>(begin: 0.45, end: 1).animate(curved),
      child: ScaleTransition(
        scale: Tween<double>(begin: 0.8, end: 1.15).animate(curved),
        child: Container(
          width: 11,
          height: 11,
          decoration: const BoxDecoration(
            color: kBreezeMalva,
            shape: BoxShape.circle,
            boxShadow: [
              BoxShadow(color: kBreezeMalva, blurRadius: 6, spreadRadius: 1),
            ],
          ),
        ),
      ),
    );
  }
}