import 'package:flutter_test/flutter_test.dart';

import 'package:inputvoice/src/l10n/strings.dart';
import 'package:inputvoice/src/overlay/overlay_screen.dart';
import 'package:inputvoice/src/tray/tray_controller.dart';

// trayStateLabel drives the tray tooltip and the menu status line
// (FR-08 + FR-11 + FR-13); the mapping is pure, so it gets unit coverage
// in both interface languages.
void main() {
  const en = BreezeStrings.en;
  const es = BreezeStrings.es;

  test('paused wins over any overlay phase (both languages)', () {
    expect(
      trayStateLabel(const PhaseListening(), paused: true, strings: en),
      en.statePaused,
    );
    expect(
      trayStateLabel(const PhaseHidden(), paused: true, strings: es),
      es.statePaused,
    );
  });

  test('cycle phases map to the localized labels', () {
    expect(trayStateLabel(const PhaseHidden(), paused: false, strings: en), en.stateActive);
    expect(trayStateLabel(const PhaseListening(), paused: false, strings: en), en.overlayListening);
    expect(trayStateLabel(const PhaseTranscribing(), paused: false, strings: es), es.overlayTranscribing);
    expect(trayStateLabel(const PhaseInjecting(), paused: false, strings: es), es.overlayPasting);
    expect(trayStateLabel(const PhaseFatal('x'), paused: false, strings: en), en.stateFatal);
  });

  test('overlay state labels are capitalized in both languages', () {
    for (final s in [en, es]) {
      for (final label in [s.overlayListening, s.overlayTranscribing, s.overlayPasting, s.overlayStarting]) {
        expect(label[0], label[0].toUpperCase(),
            reason: 'state labels must start uppercase (user request): $label');
      }
    }
  });

  test('short error keeps the message', () {
    expect(
      trayStateLabel(const PhaseError('mic busy'), paused: false, strings: en),
      '${en.errorPrefix}mic busy',
    );
  });

  test('long error is truncated for tooltip-sized surfaces', () {
    final label = trayStateLabel(PhaseError('a' * 100), paused: false, strings: en);
    expect(label.startsWith(en.errorPrefix), isTrue);
    expect(label.length, lessThanOrEqualTo(48));
  });

  group('trayIconAsset', () {
    test('paused wins over phase, both themes', () {
      expect(
        trayIconAsset(const PhaseListening(), paused: true, lightTaskbar: true),
        'assets/breeze_paused.ico',
      );
      expect(
        trayIconAsset(const PhaseListening(), paused: true, lightTaskbar: false),
        'assets/breeze_paused_light.ico',
      );
    });

    test('in-flight phases map to recording, idle otherwise', () {
      for (final phase in [
        const PhaseListening(),
        const PhaseTranscribing(),
        const PhaseInjecting(),
      ]) {
        expect(
          trayIconAsset(phase, paused: false, lightTaskbar: false),
          'assets/breeze_recording_light.ico',
        );
      }
      expect(
        trayIconAsset(const PhaseHidden(), paused: false, lightTaskbar: true),
        'assets/breeze_idle.ico',
      );
      expect(
        trayIconAsset(const PhaseError('x'), paused: false, lightTaskbar: false),
        'assets/breeze_idle_light.ico',
      );
    });

    // FR-09: the fatal / no-model tray state reuses the paused (gray)
    // icon so it is visually distinct from idle/recording without a new
    // asset. Both taskbar themes, not paused (fatal is its own phase).
    test('fatal phase reuses the paused icon, both themes', () {
      expect(
        trayIconAsset(const PhaseFatal('x'), paused: false, lightTaskbar: true),
        'assets/breeze_paused.ico',
      );
      expect(
        trayIconAsset(const PhaseFatal('x'), paused: false, lightTaskbar: false),
        'assets/breeze_paused_light.ico',
      );
    });
  });

  // FR-09: the tray exposes an on-demand "Verify model" item (key
  // verify-model) between Open logs and Quit, in both interface
  // languages. The key is a stable constant and the label is localized.
  group('verify-model menu item', () {
    test('stable dispatch key', () {
      expect(kTrayKeyVerifyModel, 'verify-model');
    });

    test('localized label exists in both languages', () {
      expect(en.menuVerifyModel, 'Verify model');
      expect(es.menuVerifyModel, 'Verificar modelo');
    });
  });
}