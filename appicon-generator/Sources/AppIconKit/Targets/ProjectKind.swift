import Foundation

/// The kind of project the icons are being generated for.
public enum ProjectKind: String, Sendable, CaseIterable {
    /// A native Xcode project with an asset catalog.
    case ios
    /// A Flutter app, driven through `flutter_launcher_icons`.
    case flutter
    /// Neither — just write a PNG.
    case raw
}

/// Works out what kind of project a directory holds.
public struct ProjectDetector {
    /// How far below `root` to look. Xcode puts the asset catalog in a group
    /// directory beside the `.xcodeproj`, and a Flutter app's `pubspec.yaml`
    /// can sit one level down if the user is standing in a repo root, so one
    /// level is enough and keeps the tool from wandering into `.build` or
    /// `node_modules`.
    static let descendLevels = 1

    private let fileManager: FileManager

    public init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
    }

    public func detect(in root: URL) -> ProjectKind {
        // Flutter is checked first and deliberately so: a Flutter app contains
        // ios/Runner/Assets.xcassets, so testing for an asset catalog first
        // would classify every Flutter app as a native iOS project.
        if findFlutterPubspec(in: root) != nil { return .flutter }
        if findAssetCatalog(in: root) != nil { return .ios }
        if findXcodeProject(in: root) != nil { return .ios }
        return .raw
    }

    /// The `pubspec.yaml` of a Flutter app at or below `root`, if there is one.
    ///
    /// A plain Dart package has a `pubspec.yaml` too, so the file also has to
    /// declare a dependency on the Flutter SDK to count.
    public func findFlutterPubspec(in root: URL) -> URL? {
        firstMatch(in: root) { url in
            guard url.lastPathComponent == "pubspec.yaml", !url.isDirectory else { return false }
            guard let contents = try? String(contentsOf: url, encoding: .utf8) else { return false }
            return contents.contains("sdk: flutter")
        }
    }

    /// The `Assets.xcassets` directory at or below `root`, if there is one.
    public func findAssetCatalog(in root: URL) -> URL? {
        firstMatch(in: root) { $0.lastPathComponent == "Assets.xcassets" && $0.isDirectory }
    }

    private func findXcodeProject(in root: URL) -> URL? {
        firstMatch(in: root) { ["xcodeproj", "xcworkspace"].contains($0.pathExtension) }
    }

    /// Breadth-first so that a shallower match always wins over a deeper one —
    /// in a Flutter app, the app's own pubspec.yaml should beat one vendored
    /// inside a subdirectory.
    private func firstMatch(in root: URL, where matches: (URL) -> Bool) -> URL? {
        var levels = [[root]]
        for level in 0...Self.descendLevels {
            guard level < levels.count else { break }
            var next: [URL] = []
            for directory in levels[level] {
                guard let children = try? contents(of: directory) else { continue }
                if let match = children.first(where: matches) { return match }
                next += children.filter { $0.isDirectory && !$0.isSkippedDirectory }
            }
            levels.append(next)
        }
        return nil
    }

    private func contents(of directory: URL) throws -> [URL] {
        try fileManager.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        )
        .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }
}

extension URL {
    var isDirectory: Bool {
        (try? resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true || hasDirectoryPath
    }

    /// Build and dependency directories, which can be enormous and never hold
    /// a project's own icon assets.
    fileprivate var isSkippedDirectory: Bool {
        [
            "build", "Build", "DerivedData", "Pods", "Carthage",
            "node_modules", "target", "vendor",
        ].contains(lastPathComponent)
    }
}
