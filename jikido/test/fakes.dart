import 'dart:async';

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

  /// Every sequence rung, in order.
  final List<BellSequence> sequences = <BellSequence>[];

  /// Every bell struck, in order. Kept alongside [sequences] because most
  /// tests care about one or the other, not both.
  final List<Bell> strikes = <Bell>[];

  /// Free-play strikes, which overlap rather than replacing each other.
  int taps = 0;
  int damps = 0;

  bool keepAliveRunning = false;
  bool ringing = false;
  bool disposed = false;
  Bell? loaded;
  double? loadedSize;

  @override
  Future<void> initialize(Bell bell, double size) async {
    if (failToInitialize) {
      throw StateError('no audio output');
    }
    loaded = bell;
    loadedSize = size;
  }

  @override
  Future<void> load(Bell bell, double size) async {
    loaded = bell;
    loadedSize = size;
  }

  @override
  Future<void> strike(Bell bell, double size, BellSequence sequence) async {
    strikes.add(bell);
    sequences.add(sequence);
    loaded = bell;
    loadedSize = size;
    ringing = true;
  }

  @override
  Future<void> tap(Bell bell, double size) async {
    taps++;
    loaded = bell;
    loadedSize = size;
    ringing = true;
  }

  @override
  Future<void> damp() async {
    damps++;
    ringing = false;
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
  /// Makes [schedule] throw, standing in for a platform that refuses the
  /// alarm — a denied permission, or a scheduling API that objects to being
  /// called while a permission dialog is still up.
  bool failToSchedule = false;

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
    if (failToSchedule) {
      throw StateError('notifications not permitted');
    }
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
  /// Makes [start] throw, standing in for Android 14 refusing to start a
  /// foreground service while the app is not in the foreground.
  bool failToStart = false;

  /// Held by [start] until it completes, standing in for a platform call
  /// that takes its time — or never returns at all.
  Completer<void>? startBlocker;

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
    if (startBlocker != null) {
      await startBlocker!.future;
    }
    if (failToStart) {
      throw StateError('foreground service start not allowed');
    }
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
