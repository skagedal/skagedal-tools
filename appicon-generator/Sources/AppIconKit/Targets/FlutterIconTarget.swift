import Foundation

/// Writes icon source images and a `flutter_launcher_icons.yaml` into a Flutter
/// app.
///
/// The icons themselves are left to
/// [flutter_launcher_icons](https://pub.dev/packages/flutter_launcher_icons),
/// which is the package the Flutter community standardised on: it knows the
/// Android density ladder and the adaptive-icon XML, and regenerating from the
/// committed config keeps working after this tool has been forgotten about.
public struct FlutterIconTarget: IconTarget {
    /// The source images written into the app's asset directory. Their names
    /// spell out how each is drawn, because that is what determines which
    /// `flutter_launcher_icons` key can point at them.
    enum SourceImage: Sendable {
        /// Opaque, on the background colour. iOS light appearance and the
        /// Android legacy launcher icon.
        case opaque
        /// The glyph on transparency. iOS dark appearance, and the foreground
        /// layer of the Android adaptive icon.
        case transparent
        /// Greyscale on black. iOS tinted appearance.
        case tinted
        /// Greyscale on transparency. The monochrome layer Android 13+ themed
        /// icons use as an alpha mask.
        case monochrome

        var filename: String {
            switch self {
            case .opaque: "icon.png"
            case .transparent: "icon-transparent.png"
            case .tinted: "icon-tinted.png"
            case .monochrome: "icon-monochrome.png"
            }
        }

        var style: IconStyle {
            switch self {
            case .opaque: .opaqueColor
            case .transparent: .transparentColor
            case .tinted: .grayscaleOnBlack
            case .monochrome: .grayscaleTransparent
            }
        }
    }

    public enum Error: LocalizedError {
        case noFlutterProjectFound(searchedFrom: URL)

        public var errorDescription: String? {
            switch self {
            case .noFlutterProjectFound(let root):
                """
                No Flutter app found in \(root.relativeDisplayPath) or one level below it \
                (looked for a pubspec.yaml depending on the Flutter SDK).
                """
            }
        }
    }

    /// Source resolution for every generated image. `flutter_launcher_icons`
    /// downscales from these, so one size is enough.
    static let sourceSizeInPixels = 1024

    /// Which iOS appearances to configure. `light` is always written; `dark`
    /// and `tinted` add the iOS 18 variants, and `tinted` additionally sets up
    /// Android's themed-icon monochrome layer, which is its counterpart.
    public var appearances: [IconAppearance]
    /// The colour behind the Android adaptive icon's foreground layer. Should
    /// match the renderer's background so both platforms look the same.
    public var backgroundColor: IconColor
    /// Where to put the source images, relative to the app directory.
    public var assetDirectory: String
    /// The app directory, the one holding `pubspec.yaml`. Discovered when nil.
    public var projectDirectory: URL?

    public init(
        appearances: [IconAppearance] = [.light, .dark, .tinted],
        backgroundColor: IconColor = .white,
        assetDirectory: String = "assets/icon",
        projectDirectory: URL? = nil
    ) {
        self.appearances = appearances
        self.backgroundColor = backgroundColor
        self.assetDirectory = assetDirectory
        self.projectDirectory = projectDirectory
    }

    public func plan(in root: URL) throws -> GenerationPlan {
        let project = try resolvedProjectDirectory(root: root)
        let assets = project.appendingPathComponent(assetDirectory)

        var outputs: [GenerationPlan.Output] = sourceImages.map { image in
            .image(
                url: assets.appendingPathComponent(image.filename),
                sizeInPixels: Self.sourceSizeInPixels,
                style: image.style
            )
        }
        let configuration = configuration()
        outputs.append(
            .file(url: project.appendingPathComponent("flutter_launcher_icons.yaml")) {
                Data(configuration.utf8)
            }
        )

        return GenerationPlan(
            outputs: outputs,
            nextSteps: [
                "flutter pub add --dev flutter_launcher_icons   # if it isn't a dependency yet",
                "dart run flutter_launcher_icons",
            ]
        )
    }

    // MARK: - Configuration

    /// The images the generated config refers to.
    ///
    /// `transparent` is written even when the dark appearance is not wanted,
    /// because it doubles as the Android adaptive icon's foreground layer and
    /// every Android version since 8 uses one.
    var sourceImages: [SourceImage] {
        var images: [SourceImage] = [.opaque, .transparent]
        if appearances.contains(.tinted) {
            images += [.tinted, .monochrome]
        }
        return images
    }

    private func path(_ image: SourceImage) -> String {
        "\(assetDirectory)/\(image.filename)"
    }

    /// The `flutter_launcher_icons.yaml` to write.
    ///
    /// Assembled as text rather than through a YAML encoder: the file is meant
    /// to be read and hand-edited afterwards, and the comments explaining where
    /// it came from are the most useful thing in it.
    func configuration() -> String {
        var lines = [
            "# Generated by appicon-generator. Apply it with:",
            "#",
            "#     dart run flutter_launcher_icons",
            "#",
            "# Everything below is ordinary flutter_launcher_icons config, so it is fine",
            "# to hand-edit — swapping image_path for a real icon when you have one, say.",
            "",
            "flutter_launcher_icons:",
            "  image_path: \"\(path(.opaque))\"",
            "",
            "  ios: true",
        ]

        if appearances.contains(.dark) {
            lines += [
                "  # iOS 18 dark appearance. Drawn on transparency; the system supplies",
                "  # the dark backdrop behind it.",
                "  image_path_ios_dark_transparent: \"\(path(.transparent))\"",
            ]
        }
        if appearances.contains(.tinted) {
            lines += [
                "  # iOS 18 tinted appearance. Opaque greyscale on black; the system reads",
                "  # its luminance and applies its own gradient tint.",
                "  image_path_ios_tinted_grayscale: \"\(path(.tinted))\"",
            ]
        }

        lines += [
            "",
            "  android: true",
            "  # Android 8+ adaptive icon. The foreground layer is the same transparent",
            "  # image iOS uses for dark mode; flutter_launcher_icons insets it into the",
            "  # adaptive icon's safe zone, so it survives a circular or squircle mask.",
            "  adaptive_icon_background: \"\(backgroundColor.hexString)\"",
            "  adaptive_icon_foreground: \"\(path(.transparent))\"",
        ]
        if appearances.contains(.tinted) {
            lines += [
                "  # Android 13+ themed icon, the counterpart of the iOS tinted appearance.",
                "  # Here the greyscale acts as an alpha mask, so this one is transparent.",
                "  adaptive_icon_monochrome: \"\(path(.monochrome))\"",
            ]
        }

        return lines.joined(separator: "\n") + "\n"
    }

    private func resolvedProjectDirectory(root: URL) throws -> URL {
        if let projectDirectory { return projectDirectory }
        guard let pubspec = ProjectDetector().findFlutterPubspec(in: root) else {
            throw Error.noFlutterProjectFound(searchedFrom: root)
        }
        return pubspec.deletingLastPathComponent()
    }
}
