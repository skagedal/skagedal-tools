import Foundation

/// One of the three appearances iOS 18 and later can show an app icon in.
///
/// Each maps to exactly one `IconStyle`, and those mappings are the whole
/// reason the two types are separate: Apple's requirements for the dark and
/// tinted images are specific and easy to get subtly wrong.
public enum IconAppearance: String, Sendable, CaseIterable {
    /// The default icon. Must be opaque.
    case light

    /// Shown when the home screen is in dark mode. The image is drawn on
    /// transparency; the system supplies a dark backdrop behind it.
    case dark

    /// Shown when the user tints their home screen. The image must be an
    /// opaque greyscale on black — the system reads its luminance and applies
    /// its own gradient tint.
    case tinted
}

extension IconAppearance {
    /// How the image for this appearance has to be drawn.
    public var style: IconStyle {
        switch self {
        case .light: .opaqueColor
        case .dark: .transparentColor
        case .tinted: .grayscaleOnBlack
        }
    }

    /// The value written into an asset catalog's `appearances` array. `light`
    /// has none — it is the untagged, default entry.
    var luminosityValue: String? {
        switch self {
        case .light: nil
        case .dark: "dark"
        case .tinted: "tinted"
        }
    }
}
