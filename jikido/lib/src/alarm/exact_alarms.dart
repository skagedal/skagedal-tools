import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';

/// Android's "Alarms & reminders" permission.
///
/// Since Android 12 an app has to be granted this before it may schedule an
/// alarm that doze is not allowed to defer. Jikido works without it — the
/// backstop notification just becomes approximate — so it is surfaced in
/// settings as something the user can fix rather than demanded up front.
class ExactAlarms {
  const ExactAlarms();

  /// Whether this permission is a thing on the current platform at all.
  /// iOS has no equivalent; its notifications are exact by default.
  static bool get isRelevant => !kIsWeb && Platform.isAndroid;

  Future<bool> get isGranted async {
    if (!isRelevant) {
      return true;
    }
    return FlutterForegroundTask.canScheduleExactAlarms;
  }

  /// Opens the system settings page where the permission can be granted.
  /// There is no in-app dialog for this one.
  Future<void> openSettings() async {
    if (!isRelevant) {
      return;
    }
    await FlutterForegroundTask.openAlarmsAndRemindersSettings();
  }
}
