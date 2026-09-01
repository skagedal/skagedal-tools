import Foundation

/// Writes plain PNGs, for when there is no project to put them in.
public struct RawIconTarget: IconTarget {
    /// Which appearances to write. Unlike the project targets this defaults to
    /// `light` alone — a bare PNG has no asset catalog to give the variants
    /// meaning, so they are opt-in.
    public var appearances: [IconAppearance]
    public var sizeInPixels: Int
    /// Where to write. A path ending in `.png` names the file for the light
    /// appearance, and any other appearances get a suffix beside it; a
    /// directory gets `icon.png` and friends written into it. Defaults to
    /// `icon.png` in the directory the tool was run in.
    public var output: URL?

    public init(appearances: [IconAppearance] = [.light], sizeInPixels: Int = 1024, output: URL? = nil) {
        self.appearances = appearances
        self.sizeInPixels = sizeInPixels
        self.output = output
    }

    public func plan(in root: URL) throws -> GenerationPlan {
        let (directory, baseName) = destination(root: root)
        return GenerationPlan(
            outputs: appearances.map { appearance in
                let suffix = appearance == .light ? "" : "-\(appearance.rawValue)"
                return .image(
                    url: directory.appendingPathComponent("\(baseName)\(suffix).png"),
                    sizeInPixels: sizeInPixels,
                    style: appearance.style
                )
            }
        )
    }

    private func destination(root: URL) -> (directory: URL, baseName: String) {
        guard let output else { return (root, "icon") }
        if output.pathExtension.lowercased() == "png" {
            return (output.deletingLastPathComponent(), output.deletingPathExtension().lastPathComponent)
        }
        return (output, "icon")
    }
}
