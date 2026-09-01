# jikido

A zazen timer for iOS and Android, written in Flutter.

Follows the practices of Soto Zen.

*Jikidō* (直堂) is the person in a zendo who keeps time and rings the bell.

## Using it

Pick a length and press **Sit**. A settling minute passes — long enough to
arrange yourself, and adjustable or turned off in settings — and then the
opening bell rings. The ensō fills as the period passes. The bell rings out
over the beginning of the sitting rather than before it, which is how it goes
in a zendo: the countdown starts with the first strike, so twenty minutes
means twenty minutes from the bell. The settling time is not taken out of it.

Two bells are offered, and are chosen in settings:

| Bell | |
|------|--|
| **Inkin** | The small hand bell on a stick. Bright, fades in a few seconds. |
| **Keisu** | The large standing bowl gong. Low, rings for half a minute. |

Either can be made larger or smaller. It is one control rather than two
because a real bell's pitch and how long it rings are not independent — a
heavier casting sounds lower *and* rings longer — and the synthesizer moves
them together.

The closing is two strikes rather than three, and the second is stopped: the
striker is laid on the bowl instead of being lifted away, so the ring is cut
off rather than allowed to fade.

You can also interact with a bell on its own, under the bell icon: tap the 
upper half of the screen to strike it, the lower half to rest the striker and 
stop the ring. 

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

The bells are synthesized on the device. A struck bowl bell is a sum of 
exponentially decaying inharmonic partials, and the numbers describing them 
are measured from some recordings of real bells. Three things to note:

**The pitch is not the fundamental.** An inkin's hum mode is barely audible;
the partial at 2.70 times it is more than ten times louder and is what you
hear as the note. Modelling the hum as the loudest partial — the obvious
thing to do — gives something far darker and duller than any real inkin.

**The upper partials die fast.** The third partial is gone in a third of a
second while the second is still ringing after two. That collapse from a
bright clang to a nearly pure tone is most of what makes the attack sound
like struck metal rather than a synthesizer.

**Every partial is a pair.** No bowl is perfectly circular, so each mode comes
in two a few Hz apart, and the interference between them is the slow warble a
bell has. Which of the pair is louder depends on where the striker lands, and
the measured depth matters in both directions: modes of equal level beat all
the way down to silence, which sounds like a tremolo pedal, and a single mode
sounds dead.

Synthesizing on the device is what buys the variation between strikes. Where
the striker lands is a parameter, chosen afresh for each strike, and the mode
balance, the phases and the contact noise all follow from it — so three
strikes in a row are three strikes rather than one sample played three times.

One number sets a bell's size. Frequency goes inversely with it, and because
the quality factor is held constant the ring time follows: `tau = Q / (pi f)`,
with Q measured at 20300 on the single struck inkin below — 3245 Hz ringing
with a decay time of 1.99 s. The same constant independently puts a keisu at a
twelve-second decay, which is the half-minute of ring a real one has. It is a
modelling choice rather than a law bells obey; see the caveat under
**References**.

`lib/src/audio/bell_synth.dart` is the synthesizer the app uses.
`tool/synthesize_bells.py` is the reference implementation of the same model,
and the two are held together by goldens in `test/bell_synth_test.dart` —
sample-for-sample, which is why neither uses its language's random number
generator for anything the other has to reproduce.

The script still has a job of its own: the closing-bell notification, which
the operating system plays if Jikido has been killed mid-sitting. That one has
to be a file on disk, so it cannot be synthesized on demand and is always the
default size. Re-run the script from this directory after changing the model:

```
python3 tool/synthesize_bells.py
```

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

## References

### The recordings the bell model was measured from

The numbers in `tool/synthesize_bells.py` are fitted to these, so the claims
about partial ratios and decay times in **The bells** above can be checked
against them rather than taken on trust.

| | |
|--|--|
| [Monastery Store inkin demonstration](https://vimeo.com/110573379) | Three different inkin — traditional, flattop and portable — at 2695, 3234 and 6146 Hz. Where the damped strikes are: 0.05-0.17 s to fall 20 dB, against 2.5-4.7 s for one left to ring. |
| [Inkin bell, struck once](https://www.youtube.com/watch?v=xfeBig0xfJQ) | The cleanest single strike of the set, and the source of the headline numbers: partials at 1.00, 2.70, 4.93 and 7.67, decaying in 1.99 s, 0.34 s and 0.10 s. |
| [Small rin bell](https://www.youtube.com/watch?v=sqLAyvcVQZ8) | A second bell agreeing with the first to within a couple of percent, which is why the model is not just a fit to one recording. |
| [Rin bells of three sizes](https://www.youtube.com/watch?v=YV5hzjzkvyM) | 2707, 3375 and 3492 Hz, ringing 6.5, 2.6 and 3.6 s. The lowest rings by far the longest, which is the behaviour the size control reproduces — but see the caveat below. |
| [Small inkin, struck repeatedly](https://www.youtube.com/watch?v=KGN3f-atEYU) | Strikes 1.4-1.8 s apart. The app deliberately does not copy this; see `strike_interval`. |
| [Vintage bell, struck repeatedly](https://www.youtube.com/shorts/NdeuOdg0WsQ) | A much lower bell — 208 Hz hum, partials at 654 and 1813 Hz — struck every 3.6 s. |
| [Portable inkin, waved](https://www.youtube.com/watch?v=4nhYhrU1I8s) | Waving the bell smears each partial into a cluster. Not modelled, but a good picture of what the mode pairs are doing. |

**Where the model is a choice rather than a measurement.** Holding Q constant
is what makes the size control one slider instead of two, and it gets the
direction unarguably right: across the three bowls above, the lowest rings
two and a half times longer than the highest. But their individual Q values
are 11700, 17300 and 24100, against the 20300 the model uses. Wall thickness
and alloy move Q independently of size, so real bowls do not sit on one curve.
Constant Q is the simplest rule that behaves like a bell; it is not a law
bells obey.

### Other zazen timers

| | |
|--|--|
| [Ensō — Meditation Timer & Bell](https://ensomeditationtimer.app/) ([App Store](https://apps.apple.com/us/app/ens%C5%8D-meditation-timer-bell/id840637879)) | A ring that erases itself as the session passes, and a library of recorded bells, bowls and chimes. |
| [Zenso Meditation Timer](https://appstor.io/app/zenso-meditation-timer) | Draws an animated ensō as the session elapses. |
| [Tricycle: meditation app roundup](https://tricycle.org/magazine/meditation-apps-review-enso-calm-smiling-mind/) · [timer apps for iOS](https://tricycle.org/article/meditation-timer-app/) | The wider landscape, reviewed by people who sit. |

### The ensō

An ensō (円相) is drawn in one uninterrupted brushstroke and never corrected,
so what stays on the paper is a record of the moment rather than a drawing of
a circle: thick where the brush pressed, broken where it ran dry, and stopped
where it lifted. Closed, it means completeness; left open, as `lib/src/ui/enso.dart`
draws it, it means the imperfect and the still-moving. Which is the better fit
for a period of sitting — entered once, in one movement, with no going back to
tidy it up.
