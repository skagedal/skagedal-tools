import Foundation
import Testing

@testable import AppIconKit

@Suite("Raw PNG output")
struct RawIconTargetTests {
    @Test("by default it writes one icon.png in the working directory")
    func defaults() throws {
        let temp = try TemporaryDirectory()
        let outcome = try RawIconTarget().generate(using: StubRenderer(), in: temp.url)
        #expect(outcome.writtenFiles.map(\.lastPathComponent) == ["icon.png"])
        #expect(temp.exists("icon.png"))
    }

    @Test("a .png output path names the file")
    func explicitFile() throws {
        let temp = try TemporaryDirectory()
        let target = RawIconTarget(output: temp.url.appendingPathComponent("logo.png"))
        try target.generate(using: StubRenderer(), in: temp.url)
        #expect(temp.exists("logo.png"))
    }

    @Test("a directory output path gets icon.png written into it")
    func explicitDirectory() throws {
        let temp = try TemporaryDirectory()
        let target = RawIconTarget(output: temp.url.appendingPathComponent("icons"))
        try target.generate(using: StubRenderer(), in: temp.url)
        #expect(temp.exists("icons/icon.png"))
    }

    @Test("extra appearances get suffixed beside the named file")
    func appearanceSuffixes() throws {
        let temp = try TemporaryDirectory()
        let target = RawIconTarget(
            appearances: IconAppearance.allCases,
            output: temp.url.appendingPathComponent("logo.png")
        )
        let outcome = try target.generate(using: StubRenderer(), in: temp.url)
        #expect(
            outcome.writtenFiles.map(\.lastPathComponent)
                == ["logo.png", "logo-dark.png", "logo-tinted.png"]
        )
    }

    @Test("the size is passed through to the renderer")
    func size() throws {
        let temp = try TemporaryDirectory()
        let plan = try RawIconTarget(sizeInPixels: 512).plan(in: temp.url)
        for output in plan.outputs {
            if case .image(_, let sizeInPixels, _) = output {
                #expect(sizeInPixels == 512)
            }
        }
    }
}
