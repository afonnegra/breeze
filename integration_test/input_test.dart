import 'package:flutter_test/flutter_test.dart';
import 'package:inputvoice/src/rust/api/input.dart';
import 'package:inputvoice/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

/// FR-01/FR-02 (FASE 2, Task 6): la API FRB de entrada responde desde Dart.
/// Requiere un dispositivo de entrada de audio por defecto (micrófono).
/// No se inyectan teclas: solo se verifica el ciclo start/stop del monitor
/// y el prewarm del audio sin excepciones.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('audioPrewarm inicializa la captura sin excepción', (
    WidgetTester tester,
  ) async {
    await audioPrewarm();
    // Idempotente: una segunda llamada reusa el stream existente.
    await audioPrewarm();
  });

  testWidgets('el monitor de hotkey arranca, streamea y se detiene', (
    WidgetTester tester,
  ) async {
    final events = <HotkeyEventDto>[];
    final errors = <Object>[];
    final subscription = startHotkeyMonitor().listen(
      events.add,
      onError: errors.add,
    );

    // Pequeña espera: el hook queda instalado; no se esperan eventos
    // (nadie presiona Ctrl+Win durante el test).
    await Future<void>.delayed(const Duration(milliseconds: 500));

    await stopHotkeyMonitor();
    await subscription.cancel();

    expect(errors, isEmpty,
        reason: 'el stream del monitor no debe emitir errores');
    // No se asertan eventos: el teclado real podría estar en uso.
  });

  testWidgets('stopHotkeyMonitor sin monitor activo lanza InputError', (
    WidgetTester tester,
  ) async {
    await expectLater(
      stopHotkeyMonitor(),
      throwsA(isA<InputError_MonitorNotRunning>()),
    );
  });
}
