# jikido

A zazen timer for iOS and Android, written in Flutter.

Three strikes of the bell open the sitting. Then silence for as long as you
asked for — five, fifteen, twenty minutes. Then three strikes to close it.
That is the whole app.

*Jikidō* (直堂) is the person in a zendo who keeps time and rings the bell.

## Using it

Pick a length and press **Sit**. The ensō fills as the period passes. The
opening bell rings over the beginning of the sitting rather than before it,
which is how it goes in a zendo — the countdown starts with the first strike,
so twenty minutes means twenty minutes from pressing the button.

Two bells are offered, and are chosen in settings:

| Bell | |
|------|--|
| **Inkin** | The small hand bell on a stick. Bright, fades in a few seconds. |
| **Keisu** | The large standing bowl gong. Low, rings for half a minute. |

Ending a sitting early rings nothing. The sitting did not finish.

## Making sure the bell is heard

The point of a meditation timer is that you stop paying attention to it.
Everything below exists so that the closing bell rings anyway. The layers are
independent on purpose — each one covers a different way for the others to
fail.

**A held-open audio session.** From the opening bell to the closing one, a
near-silent loop plays continuously. On iOS this is what keeps the app from
being suspended: an app with the `audio` background mode that is actually
producing audio keeps running with the screen off and the phone in a pocket.
Two players are used, one for the loop and one for the bells, so that
striking a bell never leaves a gap in the output.

**The alarm channel.** The audio session uses `AVAudioSessionCategoryPlayback`
on iOS, which ignores the ring/silent switch, and `USAGE_ALARM` on Android,
which plays at alarm volume and is let through Do Not Disturb. A useful side
effect: because the opening bell goes through the same path, you hear at the
start of the sitting exactly how loud the closing one will be.

**An Android foreground service.** Playing audio does not, by itself, stop
Android reclaiming the process. A foreground service does, and it puts the
remaining time in the notification shade. iOS needs no equivalent.

**A wall clock, not a stopwatch.** The end of the sitting is an instant, not
a countdown that gets decremented. Every tick asks what should have happened
by now, so a phone that suspended the app for a while and then let it run
again rings the bell immediately rather than finishing late by however long
it was asleep.

**A scheduled notification.** The moment a sitting starts, the operating
system is handed an alarm-clock notification carrying the bell as its sound,
set for the end of the period. If everything above fails and the app is
killed outright, that still rings. It is cancelled as soon as the app rings
the bell itself, so in the normal case you never see it. If the app comes
back long after the sitting ended, it says so rather than ringing a bell at
you minutes late.

**Keeping the screen on**, optionally. Costs battery, and is the surest of
all of them, so it is offered as an explicit choice in settings.

On Android 12 and later the scheduled notification is only exact if the
"Alarms & reminders" permission is granted. Jikido works without it — settings
offers a way to grant it, and falls back to an approximate alarm otherwise.

## The bells

The bell sounds are synthesized rather than recorded, by
`tool/synthesize_bells.py`. A struck bowl bell is a sum of exponentially
decaying inharmonic partials plus a short noise transient where the mallet
lands, and each partial is rendered with a quiet detuned twin to give the
slow beating that keeps it from sounding like an organ pipe.

Each asset holds the whole three-strike sequence, with the strikes overlapping
as they do on a real bell. Ringing is therefore a single `play()` with no
timers of Jikido's own in between.

To change how the bells sound, edit the parameters at the top of the script
and re-run it from this directory:

```
python3 tool/synthesize_bells.py
cp assets/audio/inkin.wav assets/audio/keisu.wav android/app/src/main/res/raw/
cp assets/audio/inkin.wav assets/audio/keisu.wav ios/Runner/
```

The copies are not redundant: a notification sound has to be a platform
resource, and Flutter assets are not visible to the notification system.
iOS also refuses notification sounds longer than 30 seconds, which is the
constraint that sets the length of the keisu asset.

Only the standard library is needed; there is nothing to install.

## Building

The Flutter SDK version is pinned in `.fvmrc` and installed with
[fvm](https://fvm.app), so everyone — and CI — builds against the same SDK:

```
brew install fvm     # once
fvm install          # gets the version .fvmrc names
```

Then prefix Flutter commands with `fvm`:

```
fvm flutter pub get
fvm flutter test
fvm flutter run
```

`fvm flutter analyze` and `fvm flutter test` are what the top-level `./check`
runs. To move to a newer SDK, `fvm use <version>` and commit the new `.fvmrc`;
CI reads the version straight out of that file.

Editors are pointed at the SDK through the `.fvm/versions/<version>` symlink
`fvm use` leaves in the project — in VS Code that is `dart.flutterSdkPath`,
and IntelliJ takes the same path as its Flutter SDK.

The layout follows the usual split between what can be tested off-device and
what cannot:

| | |
|--|--|
| `lib/src/session.dart` | The instants of a sitting. Pure Dart, no plugins. |
| `lib/src/sitting_controller.dart` | Drives a sitting. Takes its clock and its audio, notification and service layers by injection, so the timing can be tested with a clock the test moves by hand. |
| `lib/src/audio/` | just_audio and the audio session. |
| `lib/src/alarm/` | The scheduled notification, the foreground service, and the exact-alarm permission. |
| `lib/src/ui/` | Screens, and the ensō. |
