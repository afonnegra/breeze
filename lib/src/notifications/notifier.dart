import 'package:flutter/foundation.dart';
import 'package:local_notifier/local_notifier.dart';

/// Category of a user-facing error toast (docs 04-ux-requirements).
/// Only the flows the spec allows produce a toast; every other error
/// stays silent to avoid notification noise.
enum ErrorToastKind { microphone, gpu }

/// A ready-to-show toast: Spanish title and body with Breeze branding.
class ErrorToast {
  const ErrorToast(this.kind, this.title, this.body);

  final ErrorToastKind kind;
  final String title;
  final String body;
}

/// Maps a Rust-originated error message (from the overlay Error state or
/// a fatal startup error) to the toast the UX spec prescribes, or null
/// when the flow must stay silent: empty transcript, blocked paste and
/// injection failures are silent by design, and a corrupt or missing
/// model is already surfaced by the modal fatal panel. Pure and
/// case-insensitive, matching substrings of the literals Rust emits.
ErrorToast? errorToastFor(String message) {
  final m = message.toLowerCase();
  // GPU flow: CUDA out-of-memory and the transcription watchdog timeout.
  if (m.contains('gpu') ||
      m.contains('cuda') ||
      m.contains('vram') ||
      m.contains('out of memory') ||
      m.contains('oom') ||
      m.contains('timeout')) {
    return const ErrorToast(
      ErrorToastKind.gpu,
      'Breeze: GPU sin memoria',
      'La GPU no tiene memoria disponible. Cierra las aplicaciones que '
      'usen la tarjeta gráfica e inténtalo de nuevo.',
    );
  }
  // Microphone flow: no capture device, or the stream could not open.
  if (m.contains('audio') ||
      m.contains('mic') ||
      m.contains('device') ||
      m.contains('stream') ||
      m.contains('prewarm') ||
      m.contains('capture')) {
    return const ErrorToast(
      ErrorToastKind.microphone,
      'Breeze: micrófono no disponible',
      'No se puede acceder al micrófono. Revisa que no esté en '
      'uso por otra aplicación.',
    );
  }
  return null;
}

/// Shows Breeze error toasts through local_notifier (SnoreToast on
/// Windows) with a simple global throttle: at most one toast per
/// [throttle] window, so a burst of failing cycles cannot spam anyone.
///
/// The map-and-throttle decision ([shouldShow]) is split from the
/// platform call so it can be unit-tested with an injected clock and no
/// native dependency.
class ErrorToastNotifier {
  ErrorToastNotifier({DateTime Function()? clock})
      : _now = clock ?? DateTime.now;

  /// Minimum spacing between two toasts (anti-spam).
  static const Duration throttle = Duration(seconds: 5);

  final DateTime Function() _now;
  DateTime? _lastShown;
  bool _ready = false;

  /// Registers the app with the platform notifier. Best-effort: a
  /// failure only disables toasts, leaving dictation unaffected.
  Future<void> setup() async {
    try {
      await localNotifier.setup(appName: 'Breeze');
      _ready = true;
    } catch (e) {
      debugPrint('Breeze notifier setup failed: $e');
    }
  }

  /// Whether [message] should raise a toast now: it maps to a known
  /// flow AND the throttle window has elapsed. Advances the throttle
  /// clock when it returns true. Free of platform effects.
  bool shouldShow(String message) {
    if (errorToastFor(message) == null) {
      return false;
    }
    final now = _now();
    final last = _lastShown;
    if (last != null && now.difference(last) < throttle) {
      return false;
    }
    _lastShown = now;
    return true;
  }

  /// Shows an explicit, non-throttled toast (title + body). Used by the
  /// tray "Verify model" confirmation (FR-09): a user-initiated action
  /// deserves feedback, so it is not subject to the error throttle.
  /// No-op when the platform notifier is unavailable.
  Future<void> showToast(String title, String body) async {
    if (!_ready) {
      return;
    }
    try {
      final notification = LocalNotification(title: title, body: body);
      await notification.show();
    } catch (e) {
      debugPrint('Breeze notifier show failed: $e');
    }
  }

  /// Maps [message] to a toast and shows it, subject to the throttle.
  /// No-op when the message maps to nothing, is throttled, or the
  /// platform notifier is unavailable.
  Future<void> notify(String message) async {
    if (!shouldShow(message)) {
      return;
    }
    final toast = errorToastFor(message);
    if (toast == null || !_ready) {
      return;
    }
    try {
      final notification =
          LocalNotification(title: toast.title, body: toast.body);
      await notification.show();
    } catch (e) {
      debugPrint('Breeze notifier show failed: $e');
    }
  }
}
