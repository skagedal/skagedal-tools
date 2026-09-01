import Testing

@testable import AppIconKit

@Suite("IconColor parsing")
struct IconColorTests {
    @Test("six-digit hex, with and without the hash")
    func sixDigitHex() throws {
        #expect(IconColor(parsing: "#ff0000") == IconColor(red: 1, green: 0, blue: 0))
        #expect(IconColor(parsing: "00ff00") == IconColor(red: 0, green: 1, blue: 0))
        #expect(IconColor(parsing: "  #0000FF  ") == IconColor(red: 0, green: 0, blue: 1))
    }

    @Test("three-digit hex doubles each digit")
    func threeDigitHex() throws {
        #expect(IconColor(parsing: "#fff") == .white)
        #expect(IconColor(parsing: "#000") == .black)
        // #f00 is #ff0000, not #f00000.
        #expect(IconColor(parsing: "#f00") == IconColor(parsing: "#ff0000"))
    }

    @Test("named colours are case-insensitive")
    func namedColors() throws {
        #expect(IconColor(parsing: "white") == .white)
        #expect(IconColor(parsing: "BLACK") == .black)
        #expect(IconColor(parsing: "grey") == IconColor(parsing: "gray"))
        #expect(IconColor(parsing: "blue") != nil)
    }

    @Test(
        "rubbish is rejected",
        arguments: ["", "#", "#ff", "#fffff", "#ffffff0", "#gggggg", "octarine", "0x00ff00"]
    )
    func rejectsInvalid(input: String) throws {
        #expect(IconColor(parsing: input) == nil)
    }

    @Test("hex strings round-trip")
    func hexRoundTrip() throws {
        for name in IconColor.namedColors.keys {
            let color = try #require(IconColor(parsing: name))
            #expect(IconColor(parsing: color.hexString) == color)
        }
    }

    @Test("components are clamped to 0...1")
    func clamping() throws {
        #expect(IconColor(red: -1, green: 2, blue: 0.5) == IconColor(red: 0, green: 1, blue: 0.5))
    }

    @Test("luminance weights green most heavily")
    func luminance() throws {
        #expect(IconColor.white.luminance == 1)
        #expect(IconColor.black.luminance == 0)
        let green = IconColor(red: 0, green: 1, blue: 0)
        let blue = IconColor(red: 0, green: 0, blue: 1)
        #expect(green.luminance > blue.luminance)
    }
}
