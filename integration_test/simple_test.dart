import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:inputvoice/main.dart';
import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/overlay/overlay_screen.dart';
import 'package:integration_test/integration_test.dart';

/// FASE 4 Task 3 - basic overlay integration: the real app widget tree
/// renders every dictation phase. The native startup sequence
/// (RustLib.init, engine, orchestrator) is NOT exercised here - it
/// needs mic + model + hook and is validated by the startup smoke test
/// and the manual matrix (Task 5).
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('overlay renders every dictation phase', (tester) async {
    final controller = OverlayController(windowTitle: kOverlayWindowTitle);
    await tester.pumpWidget(InputVoiceOverlayApp(controller: controller));

    expect(find.text(BreezeStrings.en.overlayStarting), findsOneWidget);

    controller.value = const PhaseListening();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayListening), findsOneWidget);

    controller.value = const PhaseTranscribing();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayTranscribing), findsOneWidget);

    controller.value = const PhaseInjecting();
    await tester.pump();
    expect(find.text(BreezeStrings.en.overlayPasting), findsOneWidget);

    controller.value = const PhaseError('GPU ocupada');
    await tester.pump();
    expect(find.text('GPU ocupada'), findsOneWidget);

    controller.value = const PhaseHidden();
    await tester.pump();
    expect(find.byType(Text), findsNothing);

    controller.dispose();
  });
}
