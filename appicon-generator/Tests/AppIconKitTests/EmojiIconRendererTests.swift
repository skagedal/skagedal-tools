import AppKit
import Foundation
import Testing

@testable import AppIconKit

enum VerticalHalf {
    case upper
    case lower
}

@Suite("Emoji rendering")
struct EmojiIconRendererTests {
    private func render(
        _ text: String = "🐘",
        background: IconColor = .white,
        size: Int = 128,
        style: IconStyle,
        glyphScale: Double = 0.82
    ) throws -> NSBitmapImageRep {
        let data = try EmojiIconRenderer(
            text: text,
            backgroundColor: background,
            glyphScale: glyphScale
        )
        .renderPNG(sizeInPixels: size, style: style)
        return try #require(NSBitmapImageRep(data: data))
    }

    @Test("the PNG is square and the size that was asked for", arguments: [16, 128, 1024])
    func size(size: Int) throws {
        let image = try render(size: size, style: .opaqueColor)
        #expect(image.pixelsWide == size)
        #expect(image.pixelsHigh == size)
    }

    @Test("the opaque styles produce a PNG with no alpha channel at all")
    func opaqueStylesHaveNoAlphaChannel() throws {
        // Not merely "alpha is 255 everywhere": App Store validation rejects a
        // light app icon whose PNG carries an alpha channel, full or not.
        for style in IconStyle.allCases where style.isOpaque {
            let image = try render(style: style)
            #expect(!image.hasAlpha, "\(style) should have no alpha channel")
            #expect(image.samplesPerPixel == 3)
        }
    }

    @Test("the transparent styles keep their alpha channel")
    func transparentStylesKeepAlpha() throws {
        for style in IconStyle.allCases where !style.isOpaque {
            let image = try render(style: style)
            #expect(image.hasAlpha, "\(style) should have an alpha channel")
            // The corners are outside the glyph, so they must be see-through —
            // that is what lets iOS put its own dark backdrop behind the icon.
            #expect(image.rawPixel(x: 1, y: 1).alpha == 0)
        }
    }

    @Test("the opaque style fills the background with the chosen colour")
    func backgroundColorIsUsed() throws {
        let blue = try #require(IconColor(parsing: "#1d3557"))
        let image = try render(background: blue, style: .opaqueColor)
        let corner = image.rawPixel(x: 1, y: 1)

        // Exact, not approximate: '--background #1d3557' has to put #1d3557 in
        // the file, or the colour the user asked for is not the colour they get.
        #expect(corner.red == blue.red)
        #expect(corner.green == blue.green)
        #expect(corner.blue == blue.blue)
    }

    @Test("the tinted style is black behind the glyph, whatever the background")
    func tintedIgnoresBackgroundColor() throws {
        // The system reads the luminance and applies its own gradient, so a
        // mid-grey backdrop from flattening the chosen colour would wash the
        // whole tinted icon out.
        let blue = try #require(IconColor(parsing: "#1d3557"))
        let image = try render(background: blue, style: .grayscaleOnBlack)
        let corner = image.rawPixel(x: 1, y: 1)
        #expect(corner.red == 0)
        #expect(corner.green == 0)
        #expect(corner.blue == 0)
    }

    @Test("the greyscale styles leave no colour behind")
    func grayscaleIsActuallyGray() throws {
        for style in IconStyle.allCases where style.isGrayscale {
            let image = try render(style: style)
            var checked = 0
            for y in stride(from: 0, to: image.pixelsHigh, by: 7) {
                for x in stride(from: 0, to: image.pixelsWide, by: 7) {
                    let pixel = image.rawPixel(x: x, y: y)
                    guard pixel.alpha > 0 else { continue }
                    #expect(pixel.isGray, "coloured pixel at \(x),\(y) in \(style)")
                    checked += 1
                }
            }
            #expect(checked > 0, "\(style) rendered nothing to check")
        }
    }

    @Test("the colour styles keep their colour")
    func colorStylesAreNotGray() throws {
        let image = try render(style: .opaqueColor, glyphScale: 1)
        let isColorful = (0..<image.pixelsHigh).contains { y in
            (0..<image.pixelsWide).contains { x in
                !image.rawPixel(x: x, y: y).isGray
            }
        }
        #expect(isColorful)
    }

    @Test("the glyph is centred")
    func glyphIsCentred() throws {
        let image = try render(style: .transparentColor, glyphScale: 0.6)
        let bounds = try #require(image.opaqueBounds())
        let center = Double(image.pixelsWide) / 2

        // Within a pixel or two of centre in both directions. Colour emoji are
        // bitmap glyphs with no outline, so this is what catches a regression
        // in measuring them.
        #expect(abs(Double(bounds.midX) - center) < 3, "off-centre horizontally: \(bounds)")
        #expect(abs(Double(bounds.midY) - center) < 3, "off-centre vertically: \(bounds)")
    }

    @Test("glyphScale controls how much of the canvas the glyph covers")
    func glyphScaleIsHonoured() throws {
        let size = 256
        let small = try #require(
            try render(size: size, style: .transparentColor, glyphScale: 0.4).opaqueBounds()
        )
        let large = try #require(
            try render(size: size, style: .transparentColor, glyphScale: 0.9).opaqueBounds()
        )

        #expect(small.width < large.width)
        // The glyph should span roughly the requested fraction, and must not
        // spill past the canvas edge — iOS masks the corners off.
        #expect(Double(large.width) / Double(size) <= 1)
        #expect(Double(small.width) / Double(size) < 0.6)
    }

    @Test(
        "the icon is the right way up",
        arguments: [
            // Ink that sits below the baseline has to land in the lower half of
            // the canvas, and ink up by the ascender in the upper half. Core
            // Graphics draws from a bottom-left origin but its buffer stores the
            // top row first, and conflating the two flips the whole icon — which
            // no assertion about colour, size or centring notices.
            ("_", VerticalHalf.lower),
            ("'", VerticalHalf.upper),
        ]
    )
    func orientation(text: String, expected: VerticalHalf) throws {
        let size = 128
        let image = try render(text, size: size, style: .transparentColor, glyphScale: 0.82)
        let bounds = try #require(image.opaqueBounds())

        // opaqueBounds is in NSBitmapImageRep coordinates, so y grows downward.
        let isInUpperHalf = bounds.midY < Double(size) / 2
        #expect(isInUpperHalf == (expected == .upper), "\(text.debugDescription) at \(bounds)")
    }

    @Test("plain text renders too, not just emoji")
    func plainText() throws {
        let image = try render("A", style: .transparentColor)
        #expect(image.opaqueBounds() != nil)
    }

    @Test("text with no glyphs anywhere is an error rather than a blank icon")
    func unrenderableText() throws {
        #expect(throws: EmojiIconRenderer.Error.self) {
            try EmojiIconRenderer(text: "").renderPNG(sizeInPixels: 64, style: .opaqueColor)
        }
    }

    @Test("rendering twice in a row gives the same bytes")
    func isDeterministic() throws {
        // The renderer sets NSGraphicsContext.current while it draws; this is
        // what would catch it leaking that state into the next render.
        let renderer = EmojiIconRenderer(text: "🐘")
        let first = try renderer.renderPNG(sizeInPixels: 64, style: .opaqueColor)
        _ = try renderer.renderPNG(sizeInPixels: 512, style: .grayscaleOnBlack)
        let second = try renderer.renderPNG(sizeInPixels: 64, style: .opaqueColor)
        #expect(first == second)
    }
}

// MARK: - Pixel inspection

extension NSBitmapImageRep {
    /// One pixel's samples, 0...1, exactly as the PNG stores them.
    struct Pixel {
        var red: Double
        var green: Double
        var blue: Double
        var alpha: Double

        var isGray: Bool {
            abs(red - green) < 0.02 && abs(green - blue) < 0.02
        }
    }

    /// Reads raw samples rather than going through `colorAt(x:y:)`.
    ///
    /// `colorAt` hands back an `NSColor` that has been through a colour-space
    /// conversion, which shifts every channel by a percent or two — enough to
    /// make an exact-colour assertion fail against a PNG that is in fact
    /// byte-for-byte correct.
    func rawPixel(x: Int, y: Int) -> Pixel {
        var samples = [Int](repeating: 0, count: 5)
        getPixel(&samples, atX: x, y: y)
        return Pixel(
            red: Double(samples[0]) / 255,
            green: Double(samples[1]) / 255,
            blue: Double(samples[2]) / 255,
            alpha: hasAlpha ? Double(samples[3]) / 255 : 1
        )
    }

    /// The bounding box of every pixel that is not fully transparent.
    func opaqueBounds() -> CGRect? {
        var minX = pixelsWide
        var minY = pixelsHigh
        var maxX = -1
        var maxY = -1
        for y in 0..<pixelsHigh {
            for x in 0..<pixelsWide where rawPixel(x: x, y: y).alpha > 0.02 {
                minX = min(minX, x)
                minY = min(minY, y)
                maxX = max(maxX, x)
                maxY = max(maxY, y)
            }
        }
        guard maxX >= minX, maxY >= minY else { return nil }
        return CGRect(x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1)
    }
}
