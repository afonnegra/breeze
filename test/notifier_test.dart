import 'package:flutter_test/flutter_test.dart';

import 'package:inputvoice/src/notifications/notifier.dart';

// The error-to-toast mapping and the anti-spam throttle are pure, so
// they get unit coverage; the visual toast is validated by the user.
void main() {
  group('errorToastFor', () {
    test('GPU watchdog timeout maps to the GPU toast', () {
      final toast = errorToastFor('gpu timeout');
      expect(toast, isNotNull);
      expect(toast!.kind, ErrorToastKind.gpu);
      expect(toast.title.contains('GPU'), isTrue);
      expect(toast.body, isNotEmpty);
    });

    test('CUDA out-of-memory transcription failure maps to GPU', () {
      final toast = errorToastFor('transcription failed: CUDA out of memory');
      expect(toast?.kind, ErrorToastKind.gpu);
    });

    test('audio device and stream errors map to the microphone toast', () {
      for (final message in <String>[
        'no default audio input device available',
        'audio capture not pre-warmed: call audio_prewarm first',
        'failed to build input stream: device in use',
      ]) {
        final toast = errorToastFor(message);
        expect(toast, isNotNull, reason: message);
        expect(toast!.kind, ErrorToastKind.microphone, reason: message);
        expect(toast.title.toLowerCase().contains('micr'), isTrue);
      }
    });

    test('silent flows map to no toast', () {
      for (final message in <String>[
        '',
        'no focused control',
        'text injection failed: SendInput injected 0 of 4 key events',
        'model failed integrity check (size or hash mismatch)',
        'model not found in any search path',
      ]) {
        expect(errorToastFor(message), isNull, reason: message);
      }
    });
  });

  group('ErrorToastNotifier throttle', () {
    test('at most one toast per throttle window', () {
      var now = DateTime(2026, 1, 1, 12);
      final notifier = ErrorToastNotifier(clock: () => now);
      expect(notifier.shouldShow('gpu timeout'), isTrue);
      // A second error inside the 5 s window is suppressed.
      expect(notifier.shouldShow('gpu timeout'), isFalse);
      now = now.add(ErrorToastNotifier.throttle);
      expect(notifier.shouldShow('gpu timeout'), isTrue);
    });

    test('unmapped messages never show and do not arm the throttle', () {
      var now = DateTime(2026, 1, 1, 12);
      final notifier = ErrorToastNotifier(clock: () => now);
      expect(notifier.shouldShow('no focused control'), isFalse);
      // The throttle was not armed, so a real mapped error still shows.
      expect(notifier.shouldShow('gpu timeout'), isTrue);
    });
  });
}
