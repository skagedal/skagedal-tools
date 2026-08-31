import AppKit
import CoreGraphics
import CoreText
import Foundation

/// Draws an icon by centring a string — in practice a single emoji — on a
/// square canvas.
public struct EmojiIconRenderer: IconRenderer {
    /// Errors thrown while drawing.
    public enum Error: LocalizedError {
        case couldNotCreateGraphicsContext(sizeInPixels: Int)
        case couldNotCreateBitmap(sizeInPixels: Int)
        case couldNotEncodePNG
        case textHasNoGlyphs(String)

        public var errorDescription: String? {
            switch self {
            case .couldNotCreateGraphicsContext(let sizeInPixels):
                "Could not create a \(sizeInPixels)×\(sizeInPixels) context to draw into."
            case .couldNotCreateBitmap(let sizeInPixels):
                "Could not create a \(sizeInPixels)×\(sizeInPixels) bitmap to encode."
            case .couldNotEncodePNG:
                "Could not encode the drawn icon as PNG."
            case .textHasNoGlyphs(let text):
                "No font on this system has glyphs for \(text.debugDescription)."
            }
        }
    }

    /// The text to draw. Any string works, but a single emoji works best.
    public let text: String
    /// Fill colour for the styles that have an opaque colour background.
    public let backgroundColor: IconColor
    /// How much of the canvas the glyph should span, 0...1. The default leaves
    /// a margin, because iOS rounds the icon's corners and a glyph drawn right
    /// to the edge loses its extremities to the mask.
    public let glyphScale: Double

    public init(text: String, backgroundColor: IconColor = .white, glyphScale: Double = 0.82) {
        self.text = text
        self.backgroundColor = backgroundColor
        self.glyphScale = glyphScale
    }

    public func renderPNG(sizeInPixels: Int, style: IconStyle) throws -> Data {
        let context = try makeContext(sizeInPixels: sizeInPixels)
        try draw(in: context, style: style)
        if style.isGrayscale {
            flattenToGrayscale(context)
        }
        return try encodePNG(from: context, keepingAlpha: !style.isOpaque)
    }

    // MARK: - Context

    /// The colour space everything happens in.
    ///
    /// Explicitly sRGB rather than the device space AppKit would pick: the
    /// point of `--background '#1d3557'` is that the pixels come out
    /// `#1d3557`, and going through a display profile on the way in shifts
    /// them by a couple of percent per channel.
    private static let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!

    private func makeContext(sizeInPixels: Int) throws -> CGContext {
        // Always 32-bit RGBA. Core Graphics cannot make a context over packed
        // 24-bit RGB, so an alpha-less PNG is produced by dropping the channel
        // at encode time instead.
        guard
            let context = CGContext(
                data: nil,
                width: sizeInPixels,
                height: sizeInPixels,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: Self.colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
        else {
            throw Error.couldNotCreateGraphicsContext(sizeInPixels: sizeInPixels)
        }
        return context
    }

    // MARK: - Drawing

    private func draw(in context: CGContext, style: IconStyle) throws {
        let side = CGFloat(context.width)
        let canvas = CGRect(x: 0, y: 0, width: side, height: side)

        switch style {
        case .opaqueColor:
            context.setFillColor(backgroundColor.cgColor)
            context.fill(canvas)
        case .grayscaleOnBlack:
            // The tinted appearance reads luminance and applies the system's
            // own gradient, so the backdrop has to be black rather than the
            // chosen background flattened to some mid grey.
            context.setFillColor(IconColor.black.cgColor)
            context.fill(canvas)
        case .transparentColor, .grayscaleTransparent:
            break
        }

        let line = try fittedLine(canvasSize: side)
        var ascent: CGFloat = 0
        var descent: CGFloat = 0
        let advance = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, nil))

        // Centred on the advance width horizontally and on the ascent/descent
        // band vertically. Colour emoji are bitmap (sbix) glyphs with no
        // outline, so glyph path bounds come back empty for them and
        // typographic metrics are the only measurement that works.
        context.textPosition = CGPoint(
            x: (side - advance) / 2,
            y: (side - (ascent + descent)) / 2 + descent
        )
        CTLineDraw(line, context)
    }

    /// A laid-out line whose glyph box spans `glyphScale` of the canvas.
    private func fittedLine(canvasSize: CGFloat) throws -> CTLine {
        // Measured once at a reference size and scaled. Text layout is linear
        // in font size, so one measurement is enough — no search needed.
        let referenceSize: CGFloat = 100
        let reference = makeLine(fontSize: referenceSize)
        var ascent: CGFloat = 0
        var descent: CGFloat = 0
        let advance = CGFloat(CTLineGetTypographicBounds(reference, &ascent, &descent, nil))
        let referenceExtent = max(advance, ascent + descent)
        guard referenceExtent > 0 else { throw Error.textHasNoGlyphs(text) }

        return makeLine(fontSize: referenceSize * (canvasSize * glyphScale) / referenceExtent)
    }

    private func makeLine(fontSize: CGFloat) -> CTLine {
        // The system font has no emoji glyphs; Core Text cascades to Apple
        // Color Emoji when it lays the line out.
        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: NSFont.systemFont(ofSize: fontSize)]
        )
        return CTLineCreateWithAttributedString(attributed)
    }

    /// Rewrites the drawn pixels in place, replacing each one's colour with its
    /// luminance.
    ///
    /// Drawing into a greyscale context instead would push the conversion into
    /// Core Graphics, but colour emoji are bitmap glyphs and their conversion
    /// path there is not something to rely on; doing the arithmetic here is
    /// predictable and, at 1024², not measurably slower.
    private func flattenToGrayscale(_ context: CGContext) {
        guard let data = context.data else { return }
        let pixels = data.assumingMemoryBound(to: UInt8.self)
        for y in 0..<context.height {
            let row = pixels + y * context.bytesPerRow
            for x in 0..<context.width {
                let pixel = row + x * 4
                // Samples are premultiplied by alpha. Luminance is linear, so
                // the premultiplied grey is the premultiplied luminance and no
                // un-premultiplying round trip is needed.
                let color = IconColor(
                    red: Double(pixel[0]) / 255,
                    green: Double(pixel[1]) / 255,
                    blue: Double(pixel[2]) / 255
                )
                let gray = UInt8((color.luminance * 255).rounded())
                pixel[0] = gray
                pixel[1] = gray
                pixel[2] = gray
            }
        }
    }

    // MARK: - Encoding

    /// Copies the drawn pixels into a bitmap of the right shape and encodes it.
    ///
    /// Dropping the alpha channel rather than filling it matters: App Store
    /// validation rejects a light-appearance app icon whose PNG carries an
    /// alpha channel, even when every pixel in it is fully opaque.
    private func encodePNG(from context: CGContext, keepingAlpha: Bool) throws -> Data {
        let samplesPerPixel = keepingAlpha ? 4 : 3
        guard
            let source = context.data,
            let bitmap = NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: context.width,
                pixelsHigh: context.height,
                bitsPerSample: 8,
                samplesPerPixel: samplesPerPixel,
                hasAlpha: keepingAlpha,
                isPlanar: false,
                colorSpaceName: .deviceRGB,
                bytesPerRow: 0,
                bitsPerPixel: 0
            ),
            let destination = bitmap.bitmapData
        else {
            throw Error.couldNotCreateBitmap(sizeInPixels: context.width)
        }

        let pixels = source.assumingMemoryBound(to: UInt8.self)
        for y in 0..<context.height {
            // Core Graphics puts the origin at the bottom left and
            // NSBitmapImageRep at the top left, so the rows are copied in
            // reverse.
            let from = pixels + (context.height - 1 - y) * context.bytesPerRow
            let to = destination + y * bitmap.bytesPerRow
            for x in 0..<context.width {
                let sourcePixel = from + x * 4
                let destinationPixel = to + x * samplesPerPixel
                for sample in 0..<samplesPerPixel {
                    destinationPixel[sample] = sourcePixel[sample]
                }
            }
        }

        // The bytes are sRGB, so the representation has to say so or every
        // viewer will apply the wrong profile to them. Retagging reinterprets
        // the existing bytes rather than converting them.
        let tagged = bitmap.retagging(with: .sRGB) ?? bitmap
        guard let data = tagged.representation(using: .png, properties: [:]) else {
            throw Error.couldNotEncodePNG
        }
        return data
    }
}
