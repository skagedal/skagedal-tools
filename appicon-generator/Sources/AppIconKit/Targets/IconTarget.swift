import Foundation

/// Everything a target intends to write, worked out before anything is drawn.
///
/// Targets produce a plan rather than writing directly, which is what lets
/// `--dry-run` report the exact destinations without a second code path that
/// could drift out of step with the real one.
public struct GenerationPlan: Sendable {
    public enum Output: Sendable {
        /// A PNG to be drawn by the renderer.
        case image(url: URL, sizeInPixels: Int, style: IconStyle)
        /// A file whose contents the target supplies itself: a `Contents.json`,
        /// a `flutter_launcher_icons.yaml`.
        case file(url: URL, contents: @Sendable () throws -> Data)

        public var url: URL {
            switch self {
            case .image(let url, _, _), .file(let url, _): url
            }
        }
    }

    /// In the order they should be written.
    public var outputs: [Output]
    /// Human-readable follow-up steps, e.g. the command that applies a
    /// generated config. Empty when the icons are already in place.
    public var nextSteps: [String]

    public init(outputs: [Output], nextSteps: [String] = []) {
        self.outputs = outputs
        self.nextSteps = nextSteps
    }

    public var files: [URL] { outputs.map(\.url) }
}

/// What a target wrote, and anything the user has to do next.
public struct GenerationOutcome: Sendable, Equatable {
    public var writtenFiles: [URL]
    public var nextSteps: [String]

    public init(writtenFiles: [URL], nextSteps: [String] = []) {
        self.writtenFiles = writtenFiles
        self.nextSteps = nextSteps
    }
}

/// A place to put generated icons: an Xcode asset catalog, a Flutter app, or a
/// bare PNG on disk.
public protocol IconTarget: Sendable {
    /// Resolves where the icons go and what has to be written there.
    ///
    /// - Parameter root: The directory the user ran the tool in. Targets
    ///   discover the project below it unless they were given an explicit
    ///   destination.
    func plan(in root: URL) throws -> GenerationPlan
}

extension IconTarget {
    /// Carries out `plan(in:)`. Existing files are overwritten.
    public func generate(using renderer: IconRenderer, in root: URL) throws -> GenerationOutcome {
        let plan = try plan(in: root)
        let fileManager = FileManager.default
        for output in plan.outputs {
            try fileManager.createDirectory(
                at: output.url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data =
                switch output {
                case .image(_, let sizeInPixels, let style):
                    try renderer.renderPNG(sizeInPixels: sizeInPixels, style: style)
                case .file(_, let contents):
                    try contents()
                }
            try data.write(to: output.url)
        }
        return GenerationOutcome(writtenFiles: plan.files, nextSteps: plan.nextSteps)
    }
}
