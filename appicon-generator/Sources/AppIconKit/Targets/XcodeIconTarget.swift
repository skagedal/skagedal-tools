import Foundation

/// Writes an `AppIcon.appiconset` into an Xcode asset catalog.
public struct XcodeIconTarget: IconTarget {
    /// Which shape of app icon set to write.
    public enum Layout: Sendable, Equatable {
        /// One `universal` 1024×1024 image per appearance. What Xcode 14 and
        /// later generate, and what every currently supported iOS version
        /// wants: the system downscales for each context itself.
        case singleSize

        /// The old explicit set, one image per idiom, size and scale. Only
        /// needed for a catalog Xcode has never migrated — Flutter's iOS
        /// template still ships one.
        case legacySizes(AppIconIdioms)
    }

    public enum Error: LocalizedError {
        case noAssetCatalogFound(searchedFrom: URL)
        case appearancesUnsupportedByLegacyLayout([IconAppearance])

        public var errorDescription: String? {
            switch self {
            case .noAssetCatalogFound(let root):
                """
                No Assets.xcassets found in \(root.relativeDisplayPath) or one level below it. \
                Pass --output to point at a catalog, or --mode raw to just write a PNG.
                """
            case .appearancesUnsupportedByLegacyLayout(let appearances):
                """
                The \(appearances.map(\.rawValue).joined(separator: " and ")) appearance needs \
                the single-size layout, so it can't be combined with --legacy-sizes. Drop \
                --legacy-sizes, or pass --appearances light.
                """
            }
        }
    }

    /// Which appearances to emit. `Layout.legacySizes` supports `light` only.
    public var appearances: [IconAppearance]
    public var layout: Layout
    /// The `.xcassets` directory to write into. Discovered below `root` when
    /// nil.
    public var assetCatalog: URL?
    /// The name of the icon set, without the `.appiconset` extension.
    public var iconSetName: String

    public init(
        appearances: [IconAppearance] = IconAppearance.allCases,
        layout: Layout = .singleSize,
        assetCatalog: URL? = nil,
        iconSetName: String = "AppIcon"
    ) {
        self.appearances = appearances
        self.layout = layout
        self.assetCatalog = assetCatalog
        self.iconSetName = iconSetName
    }

    public func plan(in root: URL) throws -> GenerationPlan {
        let iconSet = try resolvedAssetCatalog(root: root)
            .appendingPathComponent("\(iconSetName).appiconset")
        let baseFilename = "icon"

        let contents: AppIconSetContents
        var outputs: [GenerationPlan.Output] = []

        switch layout {
        case .singleSize:
            contents = AppIconSetContents(baseFilename: baseFilename, appearances: appearances)
            outputs += appearances.map { appearance in
                .image(
                    url: iconSet.appendingPathComponent(
                        SingleSizeIcon(appearance: appearance).filename(base: baseFilename)
                    ),
                    sizeInPixels: SingleSizeIcon.sizeInPixels,
                    style: appearance.style
                )
            }
        case .legacySizes(let idioms):
            let unsupported = appearances.filter { $0 != .light }
            guard unsupported.isEmpty else {
                throw Error.appearancesUnsupportedByLegacyLayout(unsupported)
            }
            let variants = LegacyIconVariant.all.filter { $0.matches(idioms) }
            contents = AppIconSetContents(baseFilename: baseFilename, legacyVariants: variants)
            // Several variants land on the same pixel size — iPhone 40pt@3x and
            // iPad 60pt@2x are both 120px — and filenames are keyed on pixels,
            // so each distinct size is drawn and written once.
            var seen: Set<Int> = []
            outputs += variants.filter { seen.insert($0.pixels).inserted }.map { variant in
                .image(
                    url: iconSet.appendingPathComponent(variant.filename(base: baseFilename)),
                    sizeInPixels: variant.pixels,
                    style: .opaqueColor
                )
            }
        }

        outputs.append(
            .file(url: iconSet.appendingPathComponent("Contents.json")) { try contents.jsonData() }
        )
        return GenerationPlan(outputs: outputs)
    }

    private func resolvedAssetCatalog(root: URL) throws -> URL {
        if let assetCatalog { return assetCatalog }
        guard let found = ProjectDetector().findAssetCatalog(in: root) else {
            throw Error.noAssetCatalogFound(searchedFrom: root)
        }
        return found
    }
}
