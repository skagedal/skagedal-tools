import Foundation

/// Something that can draw one square icon.
///
/// Implement this and hand it to any of the targets in `Targets/` to get a
/// whole icon set out of it.
public protocol IconRenderer: Sendable {
    /// - Parameters:
    ///   - sizeInPixels: Width and height. Icons are always square.
    ///   - style: How to fill the background and whether to keep colour.
    /// - Returns: The icon encoded as a PNG.
    func renderPNG(sizeInPixels: Int, style: IconStyle) throws -> Data
}
