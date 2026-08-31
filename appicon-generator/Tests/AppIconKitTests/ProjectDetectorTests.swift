import Foundation
import Testing

@testable import AppIconKit

@Suite("Project detection")
struct ProjectDetectorTests {
    @Test("an empty directory is raw")
    func emptyDirectory() throws {
        let temp = try TemporaryDirectory()
        #expect(ProjectDetector().detect(in: temp.url) == .raw)
    }

    @Test("an asset catalog beside the project makes it an iOS project")
    func assetCatalogOneLevelDown() throws {
        let temp = try TemporaryDirectory()
        try temp.makeDirectory("MyApp.xcodeproj")
        let catalog = try temp.makeDirectory("MyApp/Assets.xcassets")
        #expect(ProjectDetector().detect(in: temp.url) == .ios)
        let found = ProjectDetector().findAssetCatalog(in: temp.url)
        // Resolved on both sides: /var is a symlink to /private/var, so the
        // directory enumeration and the URL we built come back spelled
        // differently for the same file.
        #expect(found?.resolvingSymlinksInPath().path == catalog.resolvingSymlinksInPath().path)
    }

    @Test("an xcodeproj alone is enough")
    func xcodeProjectWithoutCatalog() throws {
        let temp = try TemporaryDirectory()
        try temp.makeDirectory("MyApp.xcodeproj")
        #expect(ProjectDetector().detect(in: temp.url) == .ios)
    }

    @Test("a Flutter app is detected from its pubspec")
    func flutterApp() throws {
        let temp = try TemporaryDirectory()
        try temp.write(flutterPubspec, to: "pubspec.yaml")
        #expect(ProjectDetector().detect(in: temp.url) == .flutter)
    }

    @Test("Flutter beats the Xcode project nested inside it")
    func flutterBeatsItsOwnIOSDirectory() throws {
        // The trap this ordering exists for: every Flutter app contains
        // ios/Runner/Assets.xcassets and ios/Runner.xcodeproj, so checking for
        // an asset catalog first would call every Flutter app a native one.
        let temp = try TemporaryDirectory()
        try temp.write(flutterPubspec, to: "pubspec.yaml")
        try temp.makeDirectory("ios/Runner/Assets.xcassets")
        try temp.makeDirectory("ios/Runner.xcodeproj")
        #expect(ProjectDetector().detect(in: temp.url) == .flutter)
    }

    @Test("a plain Dart package is not a Flutter app")
    func dartPackageIsNotFlutter() throws {
        let temp = try TemporaryDirectory()
        try temp.write("name: mypackage\nenvironment:\n  sdk: ^3.12.2\n", to: "pubspec.yaml")
        #expect(ProjectDetector().detect(in: temp.url) == .raw)
    }

    @Test("nothing more than one level down counts")
    func doesNotDescendTooFar() throws {
        let temp = try TemporaryDirectory()
        try temp.makeDirectory("one/two/Assets.xcassets")
        #expect(ProjectDetector().detect(in: temp.url) == .raw)
    }

    @Test("build directories are not searched")
    func skipsBuildDirectories() throws {
        let temp = try TemporaryDirectory()
        try temp.makeDirectory("build/Assets.xcassets")
        try temp.makeDirectory("node_modules/Assets.xcassets")
        #expect(ProjectDetector().detect(in: temp.url) == .raw)
    }

    @Test("a shallower pubspec wins over a deeper one")
    func breadthFirst() throws {
        let temp = try TemporaryDirectory()
        try temp.write(flutterPubspec, to: "pubspec.yaml")
        try temp.write(flutterPubspec, to: "example/pubspec.yaml")
        let found = try #require(ProjectDetector().findFlutterPubspec(in: temp.url))
        #expect(found.deletingLastPathComponent().lastPathComponent == temp.url.lastPathComponent)
    }
}
