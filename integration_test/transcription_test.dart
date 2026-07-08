import 'package:flutter_test/flutter_test.dart';
import 'package:inputvoice/src/rust/api/transcription.dart';
import 'package:inputvoice/src/rust/frb_generated.dart';
import 'package:integration_test/integration_test.dart';

/// FR-04/FR-09 (FASE 1, Task 5): el engine se inicializa desde Dart vía FRB.
/// Requiere el modelo real en %LOCALAPPDATA%\inputVoice\models\ y GPU.
/// Pasar PCM real desde Dart llega en FASE 2 (captura); aquí basta el init.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('initEngine localiza el modelo y deja el engine listo', (
    WidgetTester tester,
  ) async {
    final modelPath = await initEngine(fullVerify: false);
    expect(modelPath, isNotEmpty);
    expect(modelPath.toLowerCase(), contains('ggml-large-v3-turbo-q5_0.bin'));

    expect(await engineIsReady(), isTrue);

    // Idempotencia (FR-04.AC-4): un segundo init NO recarga el modelo y
    // retorna la misma ruta.
    final secondPath = await initEngine(fullVerify: false);
    expect(secondPath, modelPath);
  });
}
