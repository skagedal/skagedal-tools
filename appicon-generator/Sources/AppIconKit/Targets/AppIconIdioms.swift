import Foundation

/// Which device families to emit icons for.
///
/// Only meaningful alongside `XcodeIconTarget.Layout.legacySizes`. The modern
/// single-size layout has one `universal` entry that covers every device, so
/// there is nothing to select.
public struct AppIconIdioms: OptionSet, Sendable {
    public let rawValue: Int

    public init(rawValue: Int) {
        self.rawValue = rawValue
    }

    public static let iPhone = AppIconIdioms(rawValue: 1 << 0)
    public static let iPad = AppIconIdioms(rawValue: 1 << 1)
    public static let all: AppIconIdioms = [.iPhone, .iPad]
}
