import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/overlay/overlay_screen.dart';

/// Pure-widget tests: phases are set directly on the controller's
/// ValueNotifier, so no Rust library and no native window is involved.
void main() {
  Widget host(OverlayController controller) {
    return MaterialApp(
      home: Scaffold(
        backgroundColor: Colors.transparent,
        body: OverlayScreen(controller: controller),
      ),
    );
  }

  // FR-09: the pure model-error classifier that drives the reinstall
  // guidance. Matches the Rust variant toStrings and the raw messages.
  test('isModelError classifies model failures, not others', () {
    expect(isModelError('TranscriptionError.modelMissing()'), isTrue);
    expect(isModelError('TranscriptionError.modelCorrupt()'), isTrue);
    expect(isModelError('model not found in any search path'), isTrue);
    expect(isModelError('model failed integrity check (size or hash mismatch)'),
        isTrue);
    expect(isModelError('CUDA out of memory'), isFalse);
    expect(isModelError('microphone unavailable'), isFalse);
  });

  testWidgets('starting phase shows its message', (tester) async {
    final controller = OverlayController(windowTitle: 'test-title');
    await tester.pumpWidget(host(controller));
    expect(find.text(BreezeStrings.en.overlayStarting), findsOneWidget);
    controller.dispose();
  });

  testWidgets('each cycle phase renders its label', (tester) async {
    final controller = OverlayController(windowTitle: 'test-title');
    await tester.pumpWidget(host(controller));

    controller.value = const PhaseListening();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayListening), findsOneWidget);

    controller.value = const PhaseTranscribing();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayTranscribing), findsOneWidget);

    controller.value = const PhaseInjecting();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayPasting), findsOneWidget);
    expect(find.byIcon(Icons.check_rounded), findsOneWidget);

    controller.value = const PhaseError('GPU ocupada');
    await tester.pump();
    expect(find.text('GPU ocupada'), findsOneWidget);
    expect(find.byIcon(Icons.warning_amber_rounded), findsOneWidget);

    controller.dispose();
  });

  testWidgets('hidden phase renders nothing', (tester) async {
    final controller = OverlayController(windowTitle: 'test-title');
    await tester.pumpWidget(host(controller));
    controller.value = const PhaseHidden();
    await tester.pump();
    expect(find.byType(Text), findsNothing);
    controller.dispose();
  });

  testWidgets('fatal phase shows the full message and the log hint', (
    tester,
  ) async {
    final controller = OverlayController(windowTitle: 'test-title');
    await tester.pumpWidget(host(controller));
    controller.setFatal('model missing');
    await tester.pump();
    expect(find.text(BreezeStrings.en.fatalTitle), findsOneWidget);
    expect(find.text('model missing'), findsOneWidget);
    expect(find.textContaining('logs'), findsOneWidget);
    controller.dispose();
  });

  // FR-09: a model error (the exact Rust variant toString) swaps the raw
  // message for the reinstall guidance; the title and log hint stay.
  testWidgets('fatal model error shows the reinstall guidance', (
    tester,
  ) async {
    final controller = OverlayController(windowTitle: 'test-title');
    await tester.pumpWidget(host(controller));
    controller.setFatal('TranscriptionError.modelCorrupt()');
    await tester.pump();
    expect(find.text(BreezeStrings.en.fatalTitle), findsOneWidget);
    expect(find.text(BreezeStrings.en.fatalModelBody), findsOneWidget);
    expect(find.text('TranscriptionError.modelCorrupt()'), findsNothing);
    controller.dispose();
  });
}
