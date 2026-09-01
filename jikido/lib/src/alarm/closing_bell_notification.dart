import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:flutter_timezone/flutter_timezone.dart';
import 'package:timezone/data/latest_all.dart' as tz_data;
import 'package:timezone/timezone.dart' as tz;

import '../bell.dart';
import '../session.dart';

/// The backstop for the closing bell.
///
/// Everything else Jikido does to stay alive — the audio session, the
/// foreground service, the optional wakelock — makes it *likely* that the app
/// is still running when the sitting ends. This makes it not matter. An
/// alarm-clock notification carrying the bell as its sound is scheduled by
/// the operating system the moment the sitting starts, and fires whether or
/// not Jikido is still around to see it.
///
/// It is cancelled as soon as the app rings the bell itself, so in the normal
/// case the user never sees it.
class ClosingBellNotification {
  ClosingBellNotification({FlutterLocalNotificationsPlugin? plugin})
      : _plugin = plugin ?? FlutterLocalNotificationsPlugin();

  final FlutterLocalNotificationsPlugin _plugin;

  static const int _notificationId = 1;

  /// How far past the closing bell a caller should set the backstop.
  ///
  /// Not zero, and this is the reason: when the app is alive it rings the
  /// bell itself and cancels this notification, and both of those take a
  /// moment. Firing at the exact same instant would mean the notification
  /// occasionally beat the cancellation and the user heard two bells.
  ///
  /// It is longer than [MeditationSession.staleAfter], which is the latest
  /// the app will ring on its own. Between them the two constants divide
  /// every possible ending into "the app rings" and "the notification
  /// rings", with no overlap and no gap.
  static const Duration backstopDelay = Duration(seconds: 6);

  bool _initialized = false;

  /// Whether the platform is one where a scheduled notification is available
  /// at all. On desktop and web the rest of Jikido still works.
  static bool get isSupported =>
      !kIsWeb && (Platform.isAndroid || Platform.isIOS);

  Future<void> initialize() async {
    if (_initialized || !isSupported) {
      return;
    }

    tz_data.initializeTimeZones();
    // Scheduling is done in local wall-clock time, so the timezone database
    // has to know which local that is.
    final timezone = await FlutterTimezone.getLocalTimezone();
    tz.setLocalLocation(tz.getLocation(timezone.identifier));

    await _plugin.initialize(
      settings: const InitializationSettings(
        android: AndroidInitializationSettings('@mipmap/ic_launcher'),
        iOS: DarwinInitializationSettings(
          // Asking on launch would be asking before the user knows what for.
          // Permission is requested when a sitting is first started.
          requestAlertPermission: false,
          requestSoundPermission: false,
          requestBadgePermission: false,
        ),
      ),
    );
    _initialized = true;
  }

  /// Asks for whatever the platform needs in order to make a noise at a
  /// specific time in the future.
  Future<void> requestPermissions() async {
    if (!isSupported) {
      return;
    }
    if (Platform.isIOS) {
      await _plugin
          .resolvePlatformSpecificImplementation<
              IOSFlutterLocalNotificationsPlugin>()
          ?.requestPermissions(alert: true, sound: true, badge: false);
      return;
    }
    await _plugin
        .resolvePlatformSpecificImplementation<
            AndroidFlutterLocalNotificationsPlugin>()
        ?.requestNotificationsPermission();
  }

  /// Schedules the backstop bell for [at].
  Future<void> schedule({
    required DateTime at,
    required Bell bell,
    required Duration sittingLength,
  }) async {
    if (!isSupported) {
      return;
    }
    await initialize();
    await cancel();

    final scheduledDate = tz.TZDateTime.from(at, tz.local);
    if (!scheduledDate.isAfter(tz.TZDateTime.now(tz.local))) {
      return;
    }

    // `alarmClock` is the only Android scheduling mode that doze is not
    // allowed to defer, but it needs the exact-alarm permission, which the
    // user can refuse. Rather than fail, drop to the best mode left: a bell
    // that may be a few minutes late still beats no bell.
    try {
      await _schedule(scheduledDate, bell, sittingLength,
          AndroidScheduleMode.alarmClock);
    } on PlatformException {
      await _schedule(scheduledDate, bell, sittingLength,
          AndroidScheduleMode.inexactAllowWhileIdle);
    }
  }

  Future<void> _schedule(
    tz.TZDateTime scheduledDate,
    Bell bell,
    Duration sittingLength,
    AndroidScheduleMode scheduleMode,
  ) async {
    await _plugin.zonedSchedule(
      id: _notificationId,
      title: 'Jikido',
      body: '${_describe(sittingLength)} sitting complete.',
      scheduledDate: scheduledDate,
      androidScheduleMode: scheduleMode,
      notificationDetails: NotificationDetails(
        android: AndroidNotificationDetails(
          // The sound belongs to the channel on Android, so each bell needs
          // its own channel. Renaming or re-sounding a channel after it has
          // been created has no effect, which is why the bell name is part
          // of the id rather than a property of one shared channel.
          'closing_bell_${bell.fileName}',
          'Closing bell (${bell.label})',
          channelDescription:
              'Rings the ${bell.label} when a sitting ends, even if Jikido '
              'has been shut down in the meantime.',
          importance: Importance.max,
          priority: Priority.max,
          category: AndroidNotificationCategory.alarm,
          visibility: NotificationVisibility.public,
          sound: RawResourceAndroidNotificationSound(bell.androidResourceName),
          // The alarm usage plays it at alarm volume, and Do Not Disturb
          // lets alarms through by default.
          audioAttributesUsage: AudioAttributesUsage.alarm,
          enableVibration: false,
        ),
        iOS: DarwinNotificationDetails(
          sound: bell.iosSoundFile,
          presentSound: true,
          presentAlert: true,
          presentBanner: true,
          // Time-sensitive notifications are delivered through Focus modes,
          // which is where a phone put down for zazen usually is. iOS only
          // honours this for apps carrying the time-sensitive entitlement;
          // without it the notification is delivered as a normal one rather
          // than failing, so it is worth asking for either way.
          interruptionLevel: InterruptionLevel.timeSensitive,
        ),
      ),
    );
  }

  Future<void> cancel() async {
    if (!isSupported) {
      return;
    }
    await _plugin.cancel(id: _notificationId);
  }

  static String _describe(Duration duration) {
    final minutes = duration.inMinutes;
    return minutes == 1 ? '1 minute' : '$minutes minute';
  }
}
