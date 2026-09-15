import AppIconKit
import ArgumentParser
import Foundation

@main
struct AppIconGeneratorCommand: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "appicon-generator",
        abstract: "Generate snazzy placeholder app icons from an emoji.",
        discussion: """
            Draws the emoji you give it on a square background and writes the result \
            wherever the project you are standing in wants its app icon.

            By default the mode is worked out from what is in the directory: a \
            pubspec.yaml depending on the Flutter SDK means a Flutter app, an \
            Assets.xcassets or an .xcodeproj means a native Xcode project, and \
            anything else just gets a PNG.

            EXAMPLES

              appicon-generator 🐘
                    Detect the project and generate icons for it.

              appicon-generator --background '#1d3557' 🚀
                    Draw on a dark blue background instead of white.

              appicon-generator --mode raw --size 512 --output logo.png 🎧
                    Just write one 512×512 PNG.

              appicon-generator --mode ios --legacy-sizes --appearances light 📎
                    Fill in an old-style icon set, one image per size and scale.

              appicon-generator --mode ios --icon-set AppIcon-Dev --badge DEV 🐘
                    A second icon set with a DEV band, for a development build.

            Existing files are overwritten without asking, so commit first if the \
            project already has an icon you care about.
            """,
        version: appIconGeneratorVersion
    )

    @Argument(
        help: ArgumentHelp("The emoji to draw. Any text works, but one emoji works best.", valueName: "emoji")
    )
    var emoji: String

    @Option(
        name: .long,
        help: ArgumentHelp(
            "What kind of project to generate for.",
            discussion: "auto detects; ios writes an Xcode asset catalog; flutter writes "
                + "flutter_launcher_icons config; raw writes a bare PNG.",
            valueName: "auto|ios|flutter|raw"
        )
    )
    var mode: Mode = .auto

    @Option(
        name: .long,
        help: ArgumentHelp(
            "Background colour: a hex triple like '#1d3557', or a name.",
            discussion: "Names: \(IconColor.namedColors.keys.sorted().joined(separator: ", ")).",
            valueName: "colour"
        )
    )
    var background: String = "white"

    @Option(
        name: .long,
        help: ArgumentHelp(
            "Appearances to generate, comma-separated, or 'all'.",
            discussion: "Defaults to all three for ios and flutter, and to light alone for raw.",
            valueName: "light,dark,tinted"
        )
    )
    var appearances: AppearanceSelection?

    @Option(
        name: .long,
        help: ArgumentHelp(
            "How much of the icon the emoji should span, 0–1.",
            valueName: "fraction"
        )
    )
    var glyphScale: Double = 0.82

    @Option(
        name: .long,
        help: ArgumentHelp(
            "A short label, like DEV, drawn in a band across the bottom of the icon.",
            valueName: "text"
        )
    )
    var badge: String?

    @Option(
        name: .long,
        help: ArgumentHelp("Colour of the --badge band, as for --background.", valueName: "colour")
    )
    var badgeColor: String?

    @Option(
        name: .long,
        help: ArgumentHelp(
            "ios only: name of the icon set to write, without .appiconset.",
            discussion: "Defaults to AppIcon. Give another to add an icon set beside it, "
                + "for a build configuration that selects it.",
            valueName: "name"
        )
    )
    var iconSet: String?

    @Flag(
        name: .long,
        help: ArgumentHelp(
            "ios only: write the old set with one image per idiom, size and scale, "
                + "instead of a single 1024×1024 universal icon."
        )
    )
    var legacySizes = false

    @Flag(name: .long, help: "ios --legacy-sizes only: emit iPhone sizes. Both by default.")
    var iphone = false

    @Flag(name: .long, help: "ios --legacy-sizes only: emit iPad sizes. Both by default.")
    var ipad = false

    @Option(
        name: .long,
        help: ArgumentHelp(
            "Where to write. An .xcassets directory for ios, the app directory for "
                + "flutter, a file or directory for raw.",
            valueName: "path"
        )
    )
    var output: String?

    @Option(name: .long, help: ArgumentHelp("raw only: pixel size of the PNG.", valueName: "pixels"))
    var size: Int = 1024

    @Flag(name: .long, help: "Say what would be written without writing it.")
    var dryRun = false

    // MARK: - Running

    func run() throws {
        let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        let kind = try resolvedProjectKind(root: root)
        let renderer = EmojiIconRenderer(
            text: emoji,
            backgroundColor: try parsedBackground(),
            glyphScale: try validatedGlyphScale(),
            badge: try parsedBadge()
        )
        let target = try makeTarget(kind: kind)

        if mode == .auto {
            print("Detected \(kind.rawValue) project.")
        }
        if dryRun {
            for file in try target.plan(in: root).files {
                print("Would write \(file.relativeDisplayPath)")
            }
            return
        }

        let outcome = try target.generate(using: renderer, in: root)
        for file in outcome.writtenFiles {
            print("Wrote \(file.relativeDisplayPath)")
        }
        if !outcome.nextSteps.isEmpty {
            print("\nNext:")
            for step in outcome.nextSteps {
                print("  \(step)")
            }
        }
    }

    private func resolvedProjectKind(root: URL) throws -> ProjectKind {
        switch mode {
        case .auto: ProjectDetector().detect(in: root)
        case .ios: .ios
        case .flutter: .flutter
        case .raw: .raw
        }
    }

    private func makeTarget(kind: ProjectKind) throws -> any IconTarget {
        let outputURL = output.map { URL(fileURLWithPath: $0) }
        switch kind {
        case .ios:
            return XcodeIconTarget(
                appearances: appearances?.appearances ?? IconAppearance.allCases,
                layout: legacySizes ? .legacySizes(requestedIdioms) : .singleSize,
                assetCatalog: outputURL,
                iconSetName: try validatedIconSet()
            )
        case .flutter:
            try rejectIOSOnlyFlags(for: kind)
            return FlutterIconTarget(
                appearances: appearances?.appearances ?? IconAppearance.allCases,
                backgroundColor: try parsedBackground(),
                projectDirectory: outputURL
            )
        case .raw:
            try rejectIOSOnlyFlags(for: kind)
            return RawIconTarget(
                appearances: appearances?.appearances ?? [.light],
                sizeInPixels: try validatedSize(),
                output: outputURL
            )
        }
    }

    private var requestedIdioms: AppIconIdioms {
        // Naming neither is the same as naming both, which is how the tool has
        // always behaved.
        switch (iphone, ipad) {
        case (false, false), (true, true): .all
        case (true, false): .iPhone
        case (false, true): .iPad
        }
    }

    // MARK: - Validation

    private func parsedBackground() throws -> IconColor {
        try parsedColor(background)
    }

    private func parsedColor(_ string: String) throws -> IconColor {
        guard let color = IconColor(parsing: string) else {
            throw ValidationError(
                "Could not read '\(string)' as a colour. Give a hex triple like "
                    + "'#1d3557', or one of: "
                    + IconColor.namedColors.keys.sorted().joined(separator: ", ")
                    + "."
            )
        }
        return color
    }

    private func parsedBadge() throws -> IconBadge? {
        guard let badge else {
            if badgeColor != nil {
                throw ValidationError("--badge-color needs a --badge to colour.")
            }
            return nil
        }
        let text = badge.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            throw ValidationError("--badge needs some text to show.")
        }
        return IconBadge(text: text, color: try badgeColor.map(parsedColor) ?? IconBadge.defaultColor)
    }

    private func validatedIconSet() throws -> String {
        guard let iconSet else { return "AppIcon" }
        guard !iconSet.isEmpty, !iconSet.contains("/"), !iconSet.hasSuffix(".appiconset") else {
            throw ValidationError(
                "--icon-set takes a bare name like AppIcon-Dev, without a path or .appiconset."
            )
        }
        return iconSet
    }

    private func validatedGlyphScale() throws -> Double {
        guard glyphScale > 0, glyphScale <= 1 else {
            throw ValidationError("--glyph-scale must be greater than 0 and at most 1.")
        }
        return glyphScale
    }

    private func validatedSize() throws -> Int {
        guard size > 0 else {
            throw ValidationError("--size must be a positive number of pixels.")
        }
        return size
    }

    private func rejectIOSOnlyFlags(for kind: ProjectKind) throws {
        let offenders = [
            ("--legacy-sizes", legacySizes),
            ("--iphone", iphone),
            ("--ipad", ipad),
            ("--icon-set", iconSet != nil),
        ]
        .filter(\.1).map(\.0)
        guard offenders.isEmpty else {
            throw ValidationError(
                "\(offenders.joined(separator: " and ")) only applies to --mode ios, "
                    + "not \(kind.rawValue)."
            )
        }
    }

}

/// The mode switch. A superset of `ProjectKind` with `auto` on top, so that the
/// kind stays a statement about the project and the mode a statement about what
/// the user asked for.
enum Mode: String, ExpressibleByArgument, CaseIterable {
    case auto
    case ios
    case flutter
    case raw
}

/// A comma-separated `--appearances` value.
///
/// A single value rather than a variadic option: a variadic one would swallow
/// the emoji, since that is a positional argument and there is no way to tell
/// `appicon-generator --appearances light dark 🎸` from a third appearance.
struct AppearanceSelection: ExpressibleByArgument, Equatable {
    var appearances: [IconAppearance]

    init?(argument: String) {
        let names = argument.split(separator: ",").map {
            $0.trimmingCharacters(in: .whitespaces).lowercased()
        }
        guard !names.isEmpty else { return nil }
        if names == ["all"] {
            appearances = IconAppearance.allCases
            return
        }
        var parsed: [IconAppearance] = []
        for name in names {
            guard let appearance = IconAppearance(rawValue: name) else { return nil }
            // Ordering follows IconAppearance.allCases rather than the order
            // given, so that light stays the untagged default entry in the
            // asset catalog however the flag was written.
            if !parsed.contains(appearance) { parsed.append(appearance) }
        }
        appearances = IconAppearance.allCases.filter(parsed.contains)
    }

    static var allValueStrings: [String] {
        ["all"] + IconAppearance.allCases.map(\.rawValue)
    }
}
