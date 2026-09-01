import 'package:wakelock_plus/wakelock_plus.dart';

/// Keeps the display from sleeping.
///
/// A one-method wrapper so that the controller has no static plugin call in
/// the middle of its teardown path, which is the sort of thing that makes
/// logic untestable for no good reason.
class ScreenAwake {
  const ScreenAwake();

  Future<void> set({required bool enabled}) =>
      WakelockPlus.toggle(enable: enabled);
}
