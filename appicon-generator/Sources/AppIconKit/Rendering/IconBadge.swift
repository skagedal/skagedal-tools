import Foundation

/// A short label drawn in a band across the bottom of the icon, to tell a
/// development build from the real one on a home screen that has both.
public struct IconBadge: Equatable, Sendable {
    /// The label. A few characters at most — `DEV`, `BETA` — because it has
    /// to be readable on a 60pt icon.
    public let text: String
    /// Fill colour of the band. The label is white or black, whichever reads
    /// better against it.
    public let color: IconColor

    public init(text: String, color: IconColor = IconBadge.defaultColor) {
        self.text = text
        self.color = color
    }

    public static let defaultColor = IconColor(hex: 0xff3b30)

    /// How much of the icon's height the band takes. Enough for a label that
    /// survives being shrunk to home-screen size, and no more: the band
    /// covers the bottom of the glyph.
    static let bandFraction = 0.24

    /// How much of the icon's width the label may span. The band's ends are
    /// lost to the rounded corners iOS masks the icon with, so the label stays
    /// well inside them.
    static let labelWidthFraction = 0.62

    /// How much of the band's height the label's capitals take.
    static let capHeightFraction = 0.46

    var labelColor: IconColor {
        color.luminance > 0.6 ? .black : .white
    }
}
