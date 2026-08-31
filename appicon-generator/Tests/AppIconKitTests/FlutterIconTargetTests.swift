import Foundation
import Testing

@testable import AppIconKit

@Suite("Flutter output")
struct FlutterIconTargetTests {
    private func makeApp() throws -> TemporaryDirectory {
        let temp = try TemporaryDirectory()
        try temp.write(flutterPubspec, to: "pubspec.yaml")
        return temp
    }

    @Test("all four source images are written, each in its own style")
    func sourceImages() throws {
        let temp = try makeApp()
        let plan = try FlutterIconTarget().plan(in: temp.url)

        let styles = plan.outputs.compactMap { output -> (String, IconStyle)? in
            guard case .image(let url, _, let style) = output else { return nil }
            return (url.lastPathComponent, style)
        }
        #expect(
            styles.map(\.0) == [
                "icon.png", "icon-transparent.png", "icon-tinted.png", "icon-monochrome.png",
            ]
        )
        #expect(
            styles.map(\.1) == [
                .opaqueColor, .transparentColor, .grayscaleOnBlack, .grayscaleTransparent,
            ]
        )
    }

    @Test("source images are 1024, since flutter_launcher_icons downscales")
    func sourceSize() throws {
        let temp = try makeApp()
        for output in try FlutterIconTarget().plan(in: temp.url).outputs {
            if case .image(_, let sizeInPixels, _) = output {
                #expect(sizeInPixels == 1024)
            }
        }
    }

    @Test("the config points at every image it writes, and no others")
    func configReferencesOnlyWrittenImages() throws {
        // A config naming a file that was never generated is the one failure
        // mode here that survives all the way to a broken `dart run`.
        for appearances in [[IconAppearance.light], [.light, .dark], IconAppearance.allCases] {
            let temp = try makeApp()
            let target = FlutterIconTarget(appearances: appearances)
            let outcome = try target.generate(using: StubRenderer(), in: temp.url)
            let written = Set(outcome.writtenFiles.map(\.lastPathComponent))

            for line in target.configuration().split(separator: "\n") {
                guard line.contains("assets/icon/") else { continue }
                let referenced = line.split(separator: "/").last!
                    .trimmingCharacters(in: CharacterSet(charactersIn: "\" "))
                #expect(written.contains(referenced), "\(referenced) is referenced but not written")
            }
        }
    }

    @Test("the transparent image is written even without the dark appearance")
    func transparentImageIsAlwaysWritten() throws {
        // It doubles as the Android adaptive icon's foreground layer, which is
        // wanted regardless of what iOS is doing.
        let temp = try makeApp()
        let target = FlutterIconTarget(appearances: [.light])
        let outcome = try target.generate(using: StubRenderer(), in: temp.url)

        #expect(outcome.writtenFiles.map(\.lastPathComponent).contains("icon-transparent.png"))
        #expect(target.configuration().contains("adaptive_icon_foreground:"))
        #expect(!target.configuration().contains("image_path_ios_dark_transparent:"))
    }

    @Test("the tinted appearance also sets up Android's themed icon")
    func tintedAddsMonochrome() throws {
        let temp = try makeApp()
        let withTinted = FlutterIconTarget(appearances: [.light, .tinted])
        #expect(withTinted.configuration().contains("adaptive_icon_monochrome:"))
        #expect(withTinted.configuration().contains("image_path_ios_tinted_grayscale:"))

        let without = FlutterIconTarget(appearances: [.light, .dark])
        #expect(!without.configuration().contains("adaptive_icon_monochrome:"))
        _ = temp
    }

    @Test("the adaptive icon background matches the chosen colour")
    func adaptiveBackgroundColor() throws {
        let blue = try #require(IconColor(parsing: "#1d3557"))
        let config = FlutterIconTarget(backgroundColor: blue).configuration()
        #expect(config.contains("adaptive_icon_background: \"#1d3557\""))
    }

    @Test("the config is valid YAML and nests under flutter_launcher_icons")
    func configShape() throws {
        let config = FlutterIconTarget().configuration()
        #expect(config.hasSuffix("\n"))
        #expect(config.contains("\nflutter_launcher_icons:\n"))
        // Every non-comment, non-blank line below the root key has to be
        // indented, or flutter_launcher_icons reads it as a different section.
        let body = config.components(separatedBy: "flutter_launcher_icons:\n")[1]
        for line in body.split(separator: "\n") where !line.isEmpty {
            #expect(line.hasPrefix("  "), "unindented line: \(line)")
        }
    }

    @Test("output lands next to the pubspec, not next to the working directory")
    func writesRelativeToTheApp() throws {
        let temp = try TemporaryDirectory()
        try temp.write(flutterPubspec, to: "myapp/pubspec.yaml")
        let plan = try FlutterIconTarget().plan(in: temp.url)
        #expect(plan.files.allSatisfy { $0.path.contains("/myapp/") })
    }

    @Test("the user is told how to apply the config")
    func nextSteps() throws {
        let temp = try makeApp()
        let plan = try FlutterIconTarget().plan(in: temp.url)
        #expect(plan.nextSteps.contains { $0.contains("dart run flutter_launcher_icons") })
    }

    @Test("a directory with no Flutter app is an error")
    func missingApp() throws {
        let temp = try TemporaryDirectory()
        #expect(throws: FlutterIconTarget.Error.self) {
            try FlutterIconTarget().plan(in: temp.url)
        }
    }
}

/// Returns a stand-in PNG without drawing anything.
struct StubRenderer: IconRenderer {
    func renderPNG(sizeInPixels: Int, style: IconStyle) throws -> Data {
        Data("png".utf8)
    }
}
