import Foundation

/// How a single icon image should be drawn.
///
/// This is deliberately about pixels, not about platforms: an asset catalog's
/// light/dark/tinted appearances and Android's adaptive-icon layers all reduce
/// to one of these four, and the mapping lives in the target that needs it.
public enum IconStyle: String, Sendable, CaseIterable {
    /// Full colour over an opaque background fill. The only style iOS accepts
    /// for a light-appearance app icon, which must have no alpha at all.
    case opaqueColor

    /// Full colour over transparency. What iOS wants for a dark-appearance
    /// icon — the system draws its own dark background behind it — and what
    /// Android wants for an adaptive icon's foreground layer.
    case transparentColor

    /// Greyscale over opaque black. What iOS wants for a tinted-appearance
    /// icon: the system reads the luminance and applies its own gradient tint,
    /// so the image has to be fully opaque.
    case grayscaleOnBlack

    /// Greyscale over transparency, for Android 13+ themed icons, whose
    /// monochrome layer is an alpha mask rather than a filled square.
    case grayscaleTransparent
}

extension IconStyle {
    /// Whether the rendered PNG is fully opaque.
    public var isOpaque: Bool {
        switch self {
        case .opaqueColor, .grayscaleOnBlack: true
        case .transparentColor, .grayscaleTransparent: false
        }
    }

    /// Whether colour is stripped from the drawing.
    public var isGrayscale: Bool {
        switch self {
        case .grayscaleOnBlack, .grayscaleTransparent: true
        case .opaqueColor, .transparentColor: false
        }
    }
}
