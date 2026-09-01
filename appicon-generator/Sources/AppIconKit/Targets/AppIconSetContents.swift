import Foundation

/// The `Contents.json` of an `AppIcon.appiconset`.
///
/// Keys are emitted sorted, which is also the order Xcode writes them in, so a
/// generated file diffs cleanly against one Xcode has touched.
struct AppIconSetContents: Encodable, Sendable {
    struct Appearance: Encodable, Sendable {
        let appearance = "luminosity"
        let value: String
    }

    struct Image: Encodable, Sendable {
        var appearances: [Appearance]?
        var filename: String
        var idiom: String
        var platform: String?
        var scale: String?
        var size: String
    }

    struct Info: Encodable, Sendable {
        let author = "appicon-generator"
        let version = 1
    }

    let images: [Image]
    let info = Info()

    func jsonData() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        var data = try encoder.encode(self)
        // Xcode's own Contents.json ends in a newline; matching it keeps the
        // file from showing up as modified the moment Xcode saves the catalog.
        data.append(0x0a)
        return data
    }
}

extension AppIconSetContents {
    /// The modern layout: one `universal` 1024×1024 entry per appearance. iOS
    /// derives every smaller size itself, so there is nothing else to ship.
    init(baseFilename: String, appearances: [IconAppearance]) {
        images = appearances.map { appearance in
            Image(
                appearances: appearance.luminosityValue.map { [Appearance(value: $0)] },
                filename: SingleSizeIcon(appearance: appearance).filename(base: baseFilename),
                idiom: "universal",
                platform: "ios",
                scale: nil,
                size: "1024x1024"
            )
        }
    }

    /// The pre-Xcode-14 layout: one entry per idiom, point size and scale.
    init(baseFilename: String, legacyVariants: [LegacyIconVariant]) {
        images = legacyVariants.map { variant in
            Image(
                appearances: nil,
                filename: variant.filename(base: baseFilename),
                idiom: variant.idiom.rawValue,
                platform: nil,
                scale: variant.scale.rawValue,
                size: variant.sizeString
            )
        }
    }
}

/// One image of a single-size app icon set.
struct SingleSizeIcon: Sendable {
    static let sizeInPixels = 1024

    let appearance: IconAppearance

    func filename(base: String) -> String {
        switch appearance {
        case .light: "\(base).png"
        case .dark, .tinted: "\(base)-\(appearance.rawValue).png"
        }
    }
}
