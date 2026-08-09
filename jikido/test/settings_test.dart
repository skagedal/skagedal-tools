import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/bell.dart';
import 'package:jikido/src/settings.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() => SharedPreferences.setMockInitialValues(<String, Object>{}));

  test('starts at fifteen minutes with the inkin', () async {
    final settings = await Settings.load();
    expect(settings.duration, const Duration(minutes: 15));
    expect(settings.bell, Bell.inkin);
    expect(settings.keepScreenOn, isFalse);
  });

  test('survives a round trip', () async {
    await const Settings(
      duration: Duration(minutes: 25),
      bell: Bell.keisu,
      keepScreenOn: true,
    ).save();

    final loaded = await Settings.load();
    expect(loaded.duration, const Duration(minutes: 25));
    expect(loaded.bell, Bell.keisu);
    expect(loaded.keepScreenOn, isTrue);
  });

  test('a stored duration from outside the allowed range is brought back in',
      () async {
    SharedPreferences.setMockInitialValues(<String, Object>{
      'duration_minutes': 100000,
    });
    expect((await Settings.load()).duration, Settings.maximumDuration);
  });

  test('an unknown bell name falls back rather than throwing', () async {
    SharedPreferences.setMockInitialValues(<String, Object>{
      'bell': 'densho',
    });
    expect((await Settings.load()).bell, Bell.inkin);
  });

  test('clamps to the allowed range', () {
    expect(Settings.clampDuration(Duration.zero), Settings.minimumDuration);
    expect(
      Settings.clampDuration(const Duration(days: 1)),
      Settings.maximumDuration,
    );
    expect(
      Settings.clampDuration(const Duration(minutes: 20)),
      const Duration(minutes: 20),
    );
  });

  test('every preset is a length the app will accept', () {
    for (final preset in Settings.presets) {
      expect(Settings.clampDuration(preset), preset);
    }
  });
}
