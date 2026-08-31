import Foundation

/// One entry in a pre-Xcode-14 app icon set: an idiom, a size in points and a
/// scale, which together give a pixel size.
struct LegacyIconVariant: Sendable, Hashable {
    enum Idiom: String, Sendable {
        case iphone
        case ipad
        case iosMarketing = "ios-marketing"
    }

    enum Scale: String, Sendable {
        case oneX = "1x"
        case twoX = "2x"
        case threeX = "3x"

        var factor: Double {
            switch self {
            case .oneX: 1
            case .twoX: 2
            case .threeX: 3
            }
        }
    }

    let idiom: Idiom
    /// In points. A `Double` because of the iPad's 83.5pt icon.
    let points: Double
    let scale: Scale

    var pixels: Int {
        Int((points * scale.factor).rounded())
    }

    /// The `size` string an asset catalog wants, e.g. `20x20` or `83.5x83.5`.
    /// Whole numbers are written without a decimal point, matching Xcode.
    var sizeString: String {
        let side =
            points == points.rounded()
            ? String(Int(points))
            : String(points)
        return "\(side)x\(side)"
    }

    func filename(base: String) -> String {
        "\(base)-\(pixels).png"
    }

    func matches(_ idioms: AppIconIdioms) -> Bool {
        switch idiom {
        case .iphone: idioms.contains(.iPhone)
        case .ipad: idioms.contains(.iPad)
        // The App Store icon is required whatever devices are targeted.
        case .iosMarketing: true
        }
    }
}

extension LegacyIconVariant {
    /// The full set Xcode generated before single-size app icons, kept for
    /// projects — Flutter's iOS template among them — that still carry one.
    static let all: [LegacyIconVariant] = [
        LegacyIconVariant(idiom: .iphone, points: 20, scale: .twoX),
        LegacyIconVariant(idiom: .iphone, points: 20, scale: .threeX),
        LegacyIconVariant(idiom: .iphone, points: 29, scale: .oneX),
        LegacyIconVariant(idiom: .iphone, points: 29, scale: .twoX),
        LegacyIconVariant(idiom: .iphone, points: 29, scale: .threeX),
        LegacyIconVariant(idiom: .iphone, points: 40, scale: .twoX),
        LegacyIconVariant(idiom: .iphone, points: 40, scale: .threeX),
        LegacyIconVariant(idiom: .iphone, points: 60, scale: .twoX),
        LegacyIconVariant(idiom: .iphone, points: 60, scale: .threeX),
        LegacyIconVariant(idiom: .ipad, points: 20, scale: .oneX),
        LegacyIconVariant(idiom: .ipad, points: 20, scale: .twoX),
        LegacyIconVariant(idiom: .ipad, points: 29, scale: .oneX),
        LegacyIconVariant(idiom: .ipad, points: 29, scale: .twoX),
        LegacyIconVariant(idiom: .ipad, points: 40, scale: .oneX),
        LegacyIconVariant(idiom: .ipad, points: 40, scale: .twoX),
        LegacyIconVariant(idiom: .ipad, points: 76, scale: .oneX),
        LegacyIconVariant(idiom: .ipad, points: 76, scale: .twoX),
        LegacyIconVariant(idiom: .ipad, points: 83.5, scale: .twoX),
        LegacyIconVariant(idiom: .iosMarketing, points: 1024, scale: .oneX),
    ]
}
