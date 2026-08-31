# appicon-generator

*Quickly generate snazzy placeholder app icons.*

If you make tiny app projects all the time — an idea you'd like to try out, a
toy for experimenting with a new platform feature, a micro-tool, the alpha
version of the next App Store blockbuster — your simulators and your phone fill
up with apps wearing the default placeholder icon. That's not very nice. But
making an icon, just for this? You don't have time. You're not a designer. You
just want something that looks OK for now.

Here's a tool that generates app icons from a big set of beautifully crafted
images that are already on your computer. `cd` to your project and type:

```shell
$ appicon-generator 🐘
Detected ios project.
Wrote MyApp/Assets.xcassets/AppIcon.appiconset/icon.png
Wrote MyApp/Assets.xcassets/AppIcon.appiconset/icon-dark.png
Wrote MyApp/Assets.xcassets/AppIcon.appiconset/icon-tinted.png
Wrote MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json
```

Now you have an elephant as an app icon. *Nice.*

## Modes

`--mode` picks what kind of project to generate for, and defaults to `auto`:

| Mode | Detected by | Output |
|------|-------------|--------|
| `ios` | an `Assets.xcassets` or an `.xcodeproj` | an `AppIcon.appiconset` in the asset catalog |
| `flutter` | a `pubspec.yaml` that depends on the Flutter SDK | source images plus a `flutter_launcher_icons.yaml` |
| `raw` | nothing else matched | a bare PNG |

Detection looks in the current directory and one level below it, and tries
Flutter *before* Xcode — every Flutter app contains
`ios/Runner/Assets.xcassets`, so the other order would call every Flutter app a
native one.

### `ios`

Writes the single-size icon set Xcode 14 and later produce: one universal
1024×1024 image per appearance, with iOS deriving every smaller size itself.

Alongside the light icon it writes the two iOS 18 variants, to Apple's
requirements — the dark one drawn on transparency so the system can put its own
backdrop behind it, the tinted one as opaque greyscale on black so the system
can read its luminance and apply its own gradient.

For a project whose catalog is still in the old shape — Flutter's iOS template
ships one — `--legacy-sizes` writes an image per idiom, size and scale instead.
It supports the light appearance only, and `--iphone` / `--ipad` narrow it to
one device family.

### `flutter`

Writes 1024px source images into `assets/icon/` and a
`flutter_launcher_icons.yaml` beside the `pubspec.yaml`, then tells you how to
apply them:

```shell
$ appicon-generator --background teal 🎧
Detected flutter project.
Wrote assets/icon/icon.png
Wrote assets/icon/icon-transparent.png
Wrote assets/icon/icon-tinted.png
Wrote assets/icon/icon-monochrome.png
Wrote flutter_launcher_icons.yaml

Next:
  flutter pub add --dev flutter_launcher_icons   # if it isn't a dependency yet
  dart run flutter_launcher_icons
```

The actual icon generation is left to
[flutter_launcher_icons](https://pub.dev/packages/flutter_launcher_icons),
which is what the Flutter community standardised on: it knows the Android
density ladder and the adaptive-icon XML, and a committed config keeps working
long after this tool has been forgotten about. The generated config sets up the
iOS light, dark and tinted appearances, the Android 8+ adaptive icon, and the
Android 13+ themed icon.

The command is not run for you — it's the user's tree and their package
manager. Add `flutter_launcher_icons` as a dev dependency once and the second
line is all you need on subsequent runs.

### `raw`

Writes one PNG, at `--size`, to `--output`:

```shell
$ appicon-generator --mode raw --size 512 --output logo.png 🎸
Wrote logo.png
```

Unlike the project modes this defaults to the light appearance alone; pass
`--appearances` to get the variants written beside it as `logo-dark.png` and
`logo-tinted.png`.

## Options

Run `appicon-generator --help` for the full list. The ones worth knowing about:

* `--background` takes a hex triple (`'#1d3557'`, `'#f0f'`, with or without the
  `#`) or one of Apple's system colour names. The bytes in the PNG are exactly
  the colour you asked for.
* `--appearances light,dark,tinted` — or `all` — selects which variants to
  write.
* `--glyph-scale` is how much of the icon the emoji spans, 0–1. The default of
  0.82 leaves a margin, because iOS rounds the corners off and a glyph drawn
  right to the edge loses its extremities to the mask.
* `--dry-run` prints where the icons would go without writing anything. It
  reports the same plan the real run carries out, so if a dry run succeeds the
  real one will too.

## Caveats

* Existing files are **overwritten without asking**. Commit first if the
  project already has an icon you care about.
* `--legacy-sizes` leaves any unreferenced images from a previous icon set
  where they are. Delete the `.appiconset` first if you want a clean one.
* Obviously, don't ship an app to the App Store with an emoji as its icon. They
  won't like that.

## AppIconKit

The generation logic lives in `AppIconKit`, a library target, with the CLI as a
thin shell over it. If you want to generate icon sets from something other than
an emoji, conform to `IconRenderer` and hand it to one of the targets in
`Sources/AppIconKit/Targets`.

## Building

```shell
$ swift build
$ swift test
$ ../check appicon-generator     # lint, build and test, as CI does
$ ../install appicon-generator   # release build into ~/.local/bin
```

macOS only: the drawing goes through AppKit and Core Text.
