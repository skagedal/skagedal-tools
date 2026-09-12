# jikido: showing the volume the bell will ring at

Implements [#66](https://github.com/skagedal/skagedal-tools/issues/66).

A sitting's bell should be as loud as it was yesterday. Too quiet and the
closing bell is missed, which is the one thing Jikido exists to prevent;
too loud and the opening bell is a jolt. But the phone's volume drifts
between sittings — a podcast, a call, a video in bed — and nothing on
screen says where it has got to. The first you hear of it is the opening
bell.

Jikido cannot set the volume on iOS, and should not set it on Android
either: the buttons on the side of the phone are the control people
already know. What it can do is show the level, live, before the sitting
starts, next to where it was the last time you sat.

## Functionality

### Which volume

The volume that matters is the one the bell plays at, and that differs
between the platforms because of how Jikido plays it.

On **Android** the bell uses the alarm usage, so it plays at the *alarm*
volume, not the media volume. The indicator shows the alarm volume.
Jikido also makes the volume buttons adjust the alarm volume while it is
in the foreground. Today they adjust media volume, which means a person
turning the phone up before a sitting moves a slider that has nothing to
do with the bell — the indicator would show them that, but it is better
not to set the trap at all.

On **iOS** there is one output volume for playback, and the bell's audio
session ignores the ring/silent switch, so the indicator shows the output
volume and nothing else. It is the volume of whatever the sound is going
to: with AirPods connected it is their level, which is correct, since
that is where the bell will ring.

### Where it is shown

On the home screen, while no sitting is running, between the ensō and
the row of lengths. And during the settling time, in the same place,
because that is when someone who has just pressed Sit notices the phone
is on silent-ish and reaches for the buttons. Once the opening bell has
rung it goes — nothing on screen during a sitting should ask to be looked
at, and the opening bell has by then answered the question more directly
than a bar can.

On the bell page too, along its top edge, since striking the bell on its
own is exactly how someone checks how loud it is.

It is not in the notification shade and not on the completion screen.

### What it looks like

A thin horizontal line, the width of the preset row, in the faded grey
the other quiet controls use, with the part up to the current level
drawn in paper white. A small speaker glyph sits at its left.

A short vertical tick marks the level at the last sitting's opening bell.
When the current level is within one step of the tick — Android has
fifteen or so steps; on iOS a step is one press of a volume button,
1/16 — the caption beneath reads **as last time**. Otherwise it reads
**louder than last time** or **quieter than last time**. Before any
sitting has been recorded there is no tick and no caption.

At zero the line is empty, the glyph is the muted speaker, and the
caption reads **silent — the bell will not be heard** in vermilion,
whatever last time was. That is the one state worth raising a voice
about.

It moves live when the volume buttons are pressed, without a tap or a
redraw of anything else.

### What counts as "last time"

The level is recorded when an opening bell is struck, and only then: a
sitting that was cancelled during the settling time did not ring a bell
at that level, and neither did the free-play bell. It is one number,
kept with the other settings, and overwritten each time.

### When the level cannot be read

If the platform will not say — an emulator, a simulator, a platform
channel that throws — the indicator is not shown at all. A bar stuck at
zero or at some default would be a confident wrong answer, which is worse
than no answer, and the rest of the app is unaffected.

## Implementation

### A platform channel of our own

The obvious dependency is `flutter_volume_controller`, and it is not
used. On iOS it sets volume through `MPVolumeView` and exposes
`setIOSAudioSessionCategory`, so it has audio session handling of its
own; Jikido's audio session category and
options are set deliberately in `bell_audio.dart`, and a second party
able to touch them is a risk to the one guarantee the app makes. What is
needed is a read and a change notification, which is a few dozen lines
per platform. Owning them costs less than auditing a plugin's session
handling across its upgrades.

One method channel and one event channel, both named `jikido/volume`:

- `get` returns the current level as a `double` from 0.0 to 1.0, plus
  `steps`, the number of steps on that platform, as an `int`.
- The event channel emits the same map whenever the level changes.

**iOS** (`ios/Runner/VolumeChannel.swift`, registered from
`didInitializeImplicitFlutterEngine` in `AppDelegate.swift` through
`engineBridge.pluginRegistry.registrar(forPlugin: "VolumeChannel")`):
`AVAudioSession.sharedInstance().outputVolume` for `get`, and key-value
observation of `outputVolume` for events. It reads the shared session and
never sets its category or activates it; `outputVolume` only updates
while the session is active, which `audio_session` makes it at startup.
`steps` is 16.

**Android** (`android/app/src/main/kotlin/tech/skagedal/jikido/VolumeChannel.kt`,
registered in `MainActivity.configureFlutterEngine`):
`AudioManager.getStreamVolume(STREAM_ALARM)` divided by
`getStreamMaxVolume(STREAM_ALARM)`, and a `ContentObserver` on
`Settings.System.CONTENT_URI` for events, re-reading the alarm stream on
each change and emitting only when it moved. `steps` is the max volume.
`MainActivity.onCreate` also sets `volumeControlStream =
AudioManager.STREAM_ALARM`, which is what makes the side buttons adjust
the alarm volume while Jikido is in front.

### Dart

- `lib/src/volume.dart` — `VolumeLevel { double level; int steps; }` and
  an abstract `Volume` with `Future<VolumeLevel?> read()` and
  `Stream<VolumeLevel> get changes`. `PlatformVolume` implements it over
  the channels, returning `null` from `read` and an empty stream when the
  channel throws `MissingPluginException` or `PlatformException`.
  `VolumeLevel.sameAs(other)` is the one-step comparison, and lives here
  as plain Dart so it is tested without a device.
- `lib/src/settings.dart` — `lastSittingVolume`, a nullable `double`,
  persisted under its own key and carried by `copyWith`.
- `lib/src/sitting_controller.dart` — takes a `Volume` by injection like
  its other layers, defaulting to `PlatformVolume()`. Where the opening
  bell is struck, both in `_engageLayers` and in `_tick`, it reads the
  level and saves it as `lastSittingVolume` without awaiting the strike
  on it. It exposes `volume` (the latest `VolumeLevel?`) and listens to
  `changes` from `initialize` until `dispose`, calling `notifyListeners`
  on each.
- `lib/src/ui/volume_indicator.dart` — the widget, taking the current
  `VolumeLevel?` and the last sitting's level, and rendering nothing for a
  null current level.
- `lib/src/ui/sitting_page.dart` — the indicator above `_PresetRow` when
  idle, and in the same slot while `isPreparing`.
- `lib/src/ui/bell_page.dart` — the indicator along the top.

`test/fakes.dart` gains `FakeVolume`, with a settable level and a
`StreamController` for changes. Tests cover: the level at the opening
bell is saved and a cancelled settling time saves nothing; the controller
passes changes through; the indicator's caption for each of silent, same,
louder and quieter and for no previous sitting; and nothing rendered when
the level is unreadable.

### Documentation

`jikido/README.md` gains a paragraph at the end of "Making sure the bell
is heard", on the indicator and on the volume buttons adjusting the
alarm volume on Android.

## Open questions

- **Showing it during the sitting.** Hiding it after the opening bell
  follows the rule that nothing on screen during a sitting asks for
  attention. But a volume turned down mid-sitting by a pocket is exactly
  the failure it would catch. A warning that only appears at zero, during
  a sitting, might be worth the exception.
- **Taking over the volume buttons on Android.** Pointing them at the
  alarm stream is right for Jikido in the foreground, but a person who
  opens Jikido while listening to something will find the buttons no
  longer turn that down. Limiting it to the home screen and the settling
  time, rather than the whole activity, is the alternative.
- **The plugin after all.** If `flutter_volume_controller` turns out to
  read iOS volume without touching the session — its source would settle
  it — the platform code here could be dropped in favour of it.
