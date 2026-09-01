import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';

/// Android's foreground service, for the length of a sitting.
///
/// Android is entitled to kill a backgrounded process whenever it wants the
/// memory, and "was playing audio" is not by itself protection. A foreground
/// service is. It also puts a countdown in the notification shade, which is
/// a pleasant way to check how long is left without unlocking the phone.
///
/// iOS needs none of this — an active audio session already keeps the app
/// running — so every method here is a no-op off Android.
class SittingService {
  const SittingService();

  static const int _serviceId = 411;

  static bool get isSupported => !kIsWeb && Platform.isAndroid;

  /// Registers the notification channel. Safe to call more than once.
  void configure() {
    if (!isSupported) {
      return;
    }
    FlutterForegroundTask.init(
      androidNotificationOptions: AndroidNotificationOptions(
        channelId: 'sitting',
        channelName: 'Sitting in progress',
        channelDescription:
            'Shown while a sitting is running, so that Android keeps Jikido '
            'alive until the closing bell.',
        // Low importance: this notification is a status line, not an event.
        // The closing bell has a channel of its own for making noise.
        channelImportance: NotificationChannelImportance.LOW,
        priority: NotificationPriority.LOW,
        onlyAlertOnce: true,
      ),
      iosNotificationOptions: const IOSNotificationOptions(
        showNotification: false,
        playSound: false,
      ),
      foregroundTaskOptions: ForegroundTaskOptions(
        // Nothing needs to happen on a schedule inside the service isolate.
        // The service exists to keep the process alive; the main isolate
        // does the timing.
        eventAction: ForegroundTaskEventAction.nothing(),
        allowWakeLock: true,
        allowWifiLock: false,
        autoRunOnBoot: false,
      ),
    );
  }

  /// Asks for permission to post the service's notification.
  Future<void> requestPermissions() async {
    if (!isSupported) {
      return;
    }
    final permission = await FlutterForegroundTask.checkNotificationPermission();
    if (permission != NotificationPermission.granted) {
      await FlutterForegroundTask.requestNotificationPermission();
    }
  }

  Future<void> start({required String text}) async {
    if (!isSupported) {
      return;
    }
    configure();
    if (await FlutterForegroundTask.isRunningService) {
      await FlutterForegroundTask.updateService(notificationText: text);
      return;
    }
    await FlutterForegroundTask.startService(
      serviceId: _serviceId,
      // The service really is playing audio for its whole life, so this is
      // the honest type — and unlike `dataSync` it has no daily time budget.
      serviceTypes: const [ForegroundServiceTypes.mediaPlayback],
      notificationTitle: 'Sitting',
      notificationText: text,
      callback: startSittingTask,
    );
  }

  Future<void> update({required String text}) async {
    if (!isSupported) {
      return;
    }
    if (!await FlutterForegroundTask.isRunningService) {
      return;
    }
    await FlutterForegroundTask.updateService(notificationText: text);
  }

  Future<void> stop() async {
    if (!isSupported) {
      return;
    }
    if (!await FlutterForegroundTask.isRunningService) {
      return;
    }
    await FlutterForegroundTask.stopService();
  }
}

/// Entry point for the service isolate. Must be top-level.
@pragma('vm:entry-point')
void startSittingTask() {
  FlutterForegroundTask.setTaskHandler(_SittingTaskHandler());
}

/// A handler that does nothing, on purpose.
///
/// All the timing lives in the main isolate, which the service keeps alive
/// simply by existing. Duplicating the clock in here would mean two things
/// that could disagree about when the bell is due.
class _SittingTaskHandler extends TaskHandler {
  @override
  Future<void> onStart(DateTime timestamp, TaskStarter starter) async {}

  @override
  void onRepeatEvent(DateTime timestamp) {}

  @override
  Future<void> onDestroy(DateTime timestamp, bool isTimeout) async {}

  @override
  void onNotificationPressed() {
    FlutterForegroundTask.launchApp();
  }
}
