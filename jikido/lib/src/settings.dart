import 'package:shared_preferences/shared_preferences.dart';

import 'bell.dart';

/// What the user has chosen, and how it is persisted.
class Settings {
  const Settings({
    this.duration = defaultDuration,
    this.bell = Bell.inkin,
    this.keepScreenOn = false,
  });

  /// The lengths offered as one tap on the home screen. Five, fifteen and
  /// twenty minutes are the periods a beginner is most often given.
  static const List<Duration> presets = <Duration>[
    Duration(minutes: 5),
    Duration(minutes: 10),
    Duration(minutes: 15),
    Duration(minutes: 20),
    Duration(minutes: 25),
    Duration(minutes: 30),
    Duration(minutes: 40),
  ];

  static const Duration defaultDuration = Duration(minutes: 15);
  static const Duration minimumDuration = Duration(minutes: 1);
  static const Duration maximumDuration = Duration(minutes: 180);

  final Duration duration;
  final Bell bell;

  /// Keep the display awake for the whole sitting. Costs battery, but it is
  /// the single most reliable way to guarantee the app is still running when
  /// the closing bell is due, so it is offered as an explicit choice.
  final bool keepScreenOn;

  Settings copyWith({Duration? duration, Bell? bell, bool? keepScreenOn}) =>
      Settings(
        duration: duration ?? this.duration,
        bell: bell ?? this.bell,
        keepScreenOn: keepScreenOn ?? this.keepScreenOn,
      );

  static const String _durationKey = 'duration_minutes';
  static const String _bellKey = 'bell';
  static const String _keepScreenOnKey = 'keep_screen_on';

  static Future<Settings> load() async {
    final preferences = await SharedPreferences.getInstance();
    final minutes = preferences.getInt(_durationKey);
    return Settings(
      duration: minutes == null
          ? defaultDuration
          : clampDuration(Duration(minutes: minutes)),
      bell: Bell.fromName(preferences.getString(_bellKey)),
      keepScreenOn: preferences.getBool(_keepScreenOnKey) ?? false,
    );
  }

  Future<void> save() async {
    final preferences = await SharedPreferences.getInstance();
    await preferences.setInt(_durationKey, duration.inMinutes);
    await preferences.setString(_bellKey, bell.name);
    await preferences.setBool(_keepScreenOnKey, keepScreenOn);
  }

  static Duration clampDuration(Duration duration) {
    if (duration < minimumDuration) {
      return minimumDuration;
    }
    if (duration > maximumDuration) {
      return maximumDuration;
    }
    return duration;
  }
}
