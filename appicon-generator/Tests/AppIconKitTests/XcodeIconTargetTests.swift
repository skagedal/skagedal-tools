import Foundation
import Testing

@testable import AppIconKit

@Suite("Xcode asset catalog output")
struct XcodeIconTargetTests {
    /// A renderer that records what it was asked for and returns a stand-in
    /// PNG, so the target's behaviour can be checked without drawing anything.
    final class RecordingRenderer: IconRenderer, @unchecked Sendable {
        private(set) var requests: [(sizeInPixels: Int, style: IconStyle)] = []

        func renderPNG(sizeInPixels: Int, style: IconStyle) throws -> Data {
            requests.append((sizeInPixels, style))
            return Data("png".utf8)
        }
    }

    private func makeCatalog() throws -> (TemporaryDirectory, URL) {
        let temp = try TemporaryDirectory()
        let catalog = try temp.makeDirectory("MyApp/Assets.xcassets")
        return (temp, catalog)
    }

    @Test("the single-size layout writes one 1024 image per appearance")
    func singleSizeImages() throws {
        let (temp, _) = try makeCatalog()
        let renderer = RecordingRenderer()
        let outcome = try XcodeIconTarget().generate(using: renderer, in: temp.url)

        #expect(
            outcome.writtenFiles.map(\.lastPathComponent) == [
                "icon.png", "icon-dark.png", "icon-tinted.png", "Contents.json",
            ]
        )
        #expect(renderer.requests.allSatisfy { $0.sizeInPixels == 1024 })
        #expect(renderer.requests.map(\.style) == [.opaqueColor, .transparentColor, .grayscaleOnBlack])
    }

    @Test("Contents.json tags dark and tinted but not light")
    func singleSizeContents() throws {
        let (temp, _) = try makeCatalog()
        try XcodeIconTarget().generate(using: RecordingRenderer(), in: temp.url)
        let json = try decodeContents(temp, "MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json")

        #expect(json.images.count == 3)
        #expect(json.images.allSatisfy { $0.idiom == "universal" })
        #expect(json.images.allSatisfy { $0.platform == "ios" })
        #expect(json.images.allSatisfy { $0.size == "1024x1024" })
        // A scale is not just unnecessary here, it is wrong: a single-size icon
        // has no scale, and actool warns when one is present.
        #expect(json.images.allSatisfy { $0.scale == nil })
        #expect(json.images[0].appearances == nil)
        #expect(json.images[1].appearances?.first?.value == "dark")
        #expect(json.images[2].appearances?.first?.value == "tinted")
    }

    @Test("asking for fewer appearances writes fewer images")
    func lightOnly() throws {
        let (temp, _) = try makeCatalog()
        let outcome = try XcodeIconTarget(appearances: [.light])
            .generate(using: RecordingRenderer(), in: temp.url)
        #expect(outcome.writtenFiles.map(\.lastPathComponent) == ["icon.png", "Contents.json"])
    }

    @Test("the legacy layout dedupes images that share a pixel size")
    func legacyDeduplication() throws {
        let (temp, _) = try makeCatalog()
        let renderer = RecordingRenderer()
        let outcome = try XcodeIconTarget(appearances: [.light], layout: .legacySizes(.all))
            .generate(using: renderer, in: temp.url)
        let json = try decodeContents(temp, "MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json")

        // 19 catalog entries, but iPhone 40pt@3x and iPad 60pt@2x are both
        // 120px and so on, leaving 13 distinct pixel sizes to draw.
        #expect(json.images.count == 19)
        #expect(renderer.requests.count == 13)
        #expect(Set(renderer.requests.map(\.sizeInPixels)).count == 13)
        #expect(outcome.writtenFiles.count == 14)

        // Every entry in Contents.json must point at a file that got written.
        let written = Set(outcome.writtenFiles.map(\.lastPathComponent))
        #expect(json.images.allSatisfy { written.contains($0.filename) })
    }

    @Test("the legacy layout can be narrowed to one device family")
    func legacyIdioms() throws {
        let (temp, _) = try makeCatalog()
        try XcodeIconTarget(appearances: [.light], layout: .legacySizes(.iPhone))
            .generate(using: RecordingRenderer(), in: temp.url)
        let json = try decodeContents(temp, "MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json")

        #expect(!json.images.contains { $0.idiom == "ipad" })
        // The App Store icon is required whichever devices are targeted.
        #expect(json.images.contains { $0.idiom == "ios-marketing" })
    }

    @Test("the iPad's 83.5pt size keeps its decimal point")
    func fractionalPointSize() throws {
        let (temp, _) = try makeCatalog()
        try XcodeIconTarget(appearances: [.light], layout: .legacySizes(.iPad))
            .generate(using: RecordingRenderer(), in: temp.url)
        let json = try decodeContents(temp, "MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json")

        #expect(json.images.contains { $0.size == "83.5x83.5" })
        #expect(json.images.contains { $0.size == "76x76" })
        #expect(!json.images.contains { $0.size == "76.0x76.0" })
    }

    @Test("the legacy layout rejects the appearances it cannot express")
    func legacyRejectsAppearances() throws {
        let (temp, _) = try makeCatalog()
        let target = XcodeIconTarget(appearances: [.light, .dark], layout: .legacySizes(.all))
        #expect(throws: XcodeIconTarget.Error.self) {
            try target.plan(in: temp.url)
        }
    }

    @Test("a missing asset catalog is an error, not a guess")
    func missingCatalog() throws {
        let temp = try TemporaryDirectory()
        #expect(throws: XcodeIconTarget.Error.self) {
            try XcodeIconTarget().plan(in: temp.url)
        }
    }

    @Test("an explicit catalog is used as given")
    func explicitCatalog() throws {
        let temp = try TemporaryDirectory()
        let catalog = try temp.makeDirectory("Somewhere/Custom.xcassets")
        let plan = try XcodeIconTarget(assetCatalog: catalog).plan(in: temp.url)
        #expect(plan.files.allSatisfy { $0.path.hasPrefix(catalog.path) })
    }

    @Test("the plan is exactly what generate writes")
    func planMatchesWhatIsWritten() throws {
        // --dry-run reports the plan, so the two drifting apart would mean the
        // tool lying about what it is about to do.
        let (temp, _) = try makeCatalog()
        let target = XcodeIconTarget()
        let planned = try target.plan(in: temp.url).files
        let written = try target.generate(using: RecordingRenderer(), in: temp.url).writtenFiles
        #expect(planned == written)
    }

    @Test("Contents.json is written the way Xcode writes it")
    func contentsFormatting() throws {
        let (temp, _) = try makeCatalog()
        try XcodeIconTarget().generate(using: RecordingRenderer(), in: temp.url)
        let text = try temp.read("MyApp/Assets.xcassets/AppIcon.appiconset/Contents.json")

        #expect(text.hasSuffix("\n"))
        // Xcode writes a space either side of the colon and sorts its keys;
        // matching both keeps a regenerated file from showing up as a diff.
        #expect(text.contains("\"idiom\" : \"universal\""))
        #expect(text.range(of: "\"images\"")!.lowerBound < text.range(of: "\"info\"")!.lowerBound)
    }

    // MARK: - Helpers

    /// A decodable mirror of `AppIconSetContents`, which is encode-only.
    private struct DecodedContents: Decodable {
        struct Appearance: Decodable {
            let appearance: String
            let value: String
        }
        struct Image: Decodable {
            let appearances: [Appearance]?
            let filename: String
            let idiom: String
            let platform: String?
            let scale: String?
            let size: String
        }
        let images: [Image]
    }

    private func decodeContents(_ temp: TemporaryDirectory, _ path: String) throws -> DecodedContents {
        let data = try Data(contentsOf: temp.url.appendingPathComponent(path))
        return try JSONDecoder().decode(DecodedContents.self, from: data)
    }
}
