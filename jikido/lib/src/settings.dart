import 'package:shared_preferences/shared_preferences.dart';

import 'bell.dart';

/// What the user has chosen, and how it is persisted.
class Settings {
  const Settings({
    this.duration = defaultDuration,
    this.bell = Bell.inkin,
    this.bellSize = defaultBellSize,
    this.prepare = defaultPrepare,
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

  /// How large the bell is, as a multiple of its default size.
  ///
  /// One number, not two, because a real bell's pitch and how long it rings
  /// are not independent — a bigger bowl sounds lower *and* longer. The
  /// synthesizer holds the quality factor constant so that moving this moves
  /// both together, the way casting a larger bell would.
  static const double defaultBellSize = 1.0;
  static const double minimumBellSize = 0.6;
  static const double maximumBellSize = 2.0;

  /// Silence before the opening bell, for settling onto the cushion.
  ///
  /// Pressing Sit and being rung at immediately leaves no room to arrange
  /// yourself, and a period that starts while you are still shuffling has
  /// started badly. Zero turns it off.
  static const Duration defaultPrepare = Duration(minutes: 1);
  static const Duration maximumPrepare = Duration(minutes: 10);

  /// The prepare times offered in settings.
  static const List<Duration> preparePresets = <Duration>[
    Duration.zero,
    Duration(seconds: 30),
    Duration(minutes: 1),
    Duration(minutes: 2),
    Duration(minutes: 5),
  ];

  final Duration duration;
  final Bell bell;
  final double bellSize;
  final Duration prepare;

  /// Keep the display awake for the whole sitting. Costs battery, but it is
  /// the single most reliable way to guarantee the app is still running when
  /// the closing bell is due, so it is offered as an explicit choice.
  final bool keepScreenOn;

  Settings copyWith({
    Duration? duration,
    Bell? bell,
    double? bellSize,
    Duration? prepare,
    bool? keepScreenOn,
  }) =>
      Settings(
        duration: duration ?? this.duration,
        bell: bell ?? this.bell,
        bellSize: bellSize ?? this.bellSize,
        prepare: prepare ?? this.prepare,
        keepScreenOn: keepScreenOn ?? this.keepScreenOn,
      );

  static const String _durationKey = 'duration_minutes';
  static const String _bellKey = 'bell';
  static const String _bellSizeKey = 'bell_size';
  static const String _prepareKey = 'prepare_seconds';
  static const String _keepScreenOnKey = 'keep_screen_on';

  static Future<Settings> load() async {
    final preferences = await SharedPreferences.getInstance();
    final minutes = preferences.getInt(_durationKey);
    final prepareSeconds = preferences.getInt(_prepareKey);
    return Settings(
      duration: minutes == null
          ? defaultDuration
          : clampDuration(Duration(minutes: minutes)),
      bell: Bell.fromName(preferences.getString(_bellKey)),
      bellSize: clampBellSize(
          preferences.getDouble(_bellSizeKey) ?? defaultBellSize),
      prepare: prepareSeconds == null
          ? defaultPrepare
          : clampPrepare(Duration(seconds: prepareSeconds)),
      keepScreenOn: preferences.getBool(_keepScreenOnKey) ?? false,
    );
  }

  Future<void> save() async {
    final preferences = await SharedPreferences.getInstance();
    await preferences.setInt(_durationKey, duration.inMinutes);
    await preferences.setString(_bellKey, bell.name);
    await preferences.setDouble(_bellSizeKey, bellSize);
    await preferences.setInt(_prepareKey, prepare.inSeconds);
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

  static double clampBellSize(double size) =>
      size.isNaN ? defaultBellSize : size.clamp(minimumBellSize, maximumBellSize);

  static Duration clampPrepare(Duration prepare) {
    if (prepare.isNegative) {
      return Duration.zero;
    }
    if (prepare > maximumPrepare) {
      return maximumPrepare;
    }
    return prepare;
  }
}
