import CoreGraphics
import Foundation

/// An opaque sRGB colour, used for icon backgrounds.
///
/// Deliberately not `NSColor`: keeping the value type plain makes the parsing
/// testable without a graphics context, and the conversion to `CGColor`
/// happens only at the point of drawing.
public struct IconColor: Equatable, Sendable {
    /// Red component, 0...1.
    public let red: Double
    /// Green component, 0...1.
    public let green: Double
    /// Blue component, 0...1.
    public let blue: Double

    public init(red: Double, green: Double, blue: Double) {
        self.red = red.clampedToUnitRange
        self.green = green.clampedToUnitRange
        self.blue = blue.clampedToUnitRange
    }

    /// From a packed 24-bit `0xrrggbb` value.
    public init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xff) / 255,
            green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255
        )
    }

    public static let white = IconColor(red: 1, green: 1, blue: 1)
    public static let black = IconColor(red: 0, green: 0, blue: 0)
}

extension IconColor {
    /// The colour as a hex string, e.g. `#ffffff`. Round-trips through
    /// `init(parsing:)`, and is the form `flutter_launcher_icons` wants for
    /// `adaptive_icon_background`.
    public var hexString: String {
        func component(_ value: Double) -> Int { Int((value * 255).rounded()) }
        return String(format: "#%02x%02x%02x", component(red), component(green), component(blue))
    }

    var cgColor: CGColor {
        CGColor(srgbRed: red, green: green, blue: blue, alpha: 1)
    }

    /// Rec. 709 luminance, used when flattening the colour to greyscale.
    var luminance: Double {
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }
}

// MARK: - Parsing

extension IconColor {
    /// Parses a colour written as a CSS-style hex triple (`#f0f`, `#ff00ff`,
    /// with or without the leading `#`) or as one of the names in
    /// `IconColor.namedColors`. Case and surrounding whitespace are ignored.
    public init?(parsing string: String) {
        let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let named = Self.namedColors[trimmed] {
            self = named
            return
        }
        let digits = trimmed.hasPrefix("#") ? String(trimmed.dropFirst()) : trimmed
        guard digits.allSatisfy(\.isHexDigit) else { return nil }
        switch digits.count {
        case 3:
            // #rgb is shorthand for #rrggbb, so each digit is doubled.
            let values = digits.compactMap { UInt8(String($0), radix: 16) }
            guard values.count == 3 else { return nil }
            self.init(
                red: Double(values[0]) / 15,
                green: Double(values[1]) / 15,
                blue: Double(values[2]) / 15
            )
        case 6:
            guard let value = UInt32(digits, radix: 16) else { return nil }
            self.init(hex: value)
        default:
            return nil
        }
    }

    /// The colour names `init(parsing:)` accepts.
    ///
    /// Apple's system colours where there is one, so that `--background blue`
    /// looks like something that belongs on an iOS home screen rather than
    /// `#0000ff`. Written as 8-bit hex, which is what they actually are — a
    /// rounded decimal triple would not survive `hexString`.
    public static let namedColors: [String: IconColor] = [
        "white": .white,
        "black": .black,
        "red": IconColor(hex: 0xff3b30),
        "orange": IconColor(hex: 0xff9500),
        "yellow": IconColor(hex: 0xffcc00),
        "green": IconColor(hex: 0x34c759),
        "mint": IconColor(hex: 0x00c7be),
        "teal": IconColor(hex: 0x30b0c7),
        "cyan": IconColor(hex: 0x32ade6),
        "blue": IconColor(hex: 0x007aff),
        "indigo": IconColor(hex: 0x5856d6),
        "purple": IconColor(hex: 0xaf52de),
        "pink": IconColor(hex: 0xff2d55),
        "brown": IconColor(hex: 0xa2845e),
        "gray": IconColor(hex: 0x8e8e93),
        "grey": IconColor(hex: 0x8e8e93),
    ]
}

extension Double {
    fileprivate var clampedToUnitRange: Double { min(max(self, 0), 1) }
}
