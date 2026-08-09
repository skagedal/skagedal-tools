import 'package:jikido/src/alarm/closing_bell_notification.dart';
import 'package:jikido/src/alarm/sitting_service.dart';
import 'package:jikido/src/audio/bell_audio.dart';
import 'package:jikido/src/bell.dart';
import 'package:jikido/src/screen_awake.dart';

/// Records what would have been played, without going near a platform
/// channel. `implements` rather than `extends`, so that the real class's
/// `AudioPlayer` fields are never constructed.
class FakeBellAudio implements BellAudio {
  /// Makes [initialize] fail, standing in for a device with no working
  /// audio output.
  bool failToInitialize = false;

  final List<Bell> strikes = <Bell>[];
  bool keepAliveRunning = false;
  bool ringing = false;
  bool disposed = false;
  Bell? loaded;

  @override
  Future<void> initialize(Bell bell) async {
    if (failToInitialize) {
      throw StateError('no audio output');
    }
    loaded = bell;
  }

  @override
  Future<void> load(Bell bell) async {
    loaded = bell;
  }

  @override
  Future<void> strike(Bell bell) async {
    strikes.add(bell);
    ringing = true;
  }

  @override
  Future<void> silence() async {
    ringing = false;
  }

  @override
  bool get isRinging => ringing;

  @override
  Future<void> startKeepAlive() async {
    keepAliveRunning = true;
  }

  @override
  Future<void> stopKeepAlive() async {
    keepAliveRunning = false;
  }

  @override
  Future<void> dispose() async {
    disposed = true;
  }
}

class FakeClosingBellNotification implements ClosingBellNotification {
  DateTime? scheduledFor;
  Bell? scheduledBell;
  int cancelCount = 0;
  bool permissionsRequested = false;

  bool get isScheduled => scheduledFor != null;

  @override
  Future<void> initialize() async {}

  @override
  Future<void> requestPermissions() async {
    permissionsRequested = true;
  }

  @override
  Future<void> schedule({
    required DateTime at,
    required Bell bell,
    required Duration sittingLength,
  }) async {
    scheduledFor = at;
    scheduledBell = bell;
  }

  @override
  Future<void> cancel() async {
    cancelCount++;
    scheduledFor = null;
    scheduledBell = null;
  }
}

class FakeSittingService implements SittingService {
  bool running = false;
  String? text;
  final List<String> texts = <String>[];
  bool permissionsRequested = false;

  @override
  void configure() {}

  @override
  Future<void> requestPermissions() async {
    permissionsRequested = true;
  }

  @override
  Future<void> start({required String text}) async {
    running = true;
    this.text = text;
    texts.add(text);
  }

  @override
  Future<void> update({required String text}) async {
    this.text = text;
    texts.add(text);
  }

  @override
  Future<void> stop() async {
    running = false;
  }
}

class FakeScreenAwake implements ScreenAwake {
  bool awake = false;

  @override
  Future<void> set({required bool enabled}) async {
    awake = enabled;
  }
}

/// A clock the test moves by hand.
class TestClock {
  TestClock(this.now);

  DateTime now;

  DateTime call() => now;

  void advance(Duration by) => now = now.add(by);
}
