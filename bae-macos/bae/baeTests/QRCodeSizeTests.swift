import SwiftUI
import Testing

@testable import bae

/// A QR code is drawn as wide as its payload needs, never at a side the
/// caller picked: the Add device code carries the S3 key, and a phone camera
/// cannot read its modules when they are drawn smaller than the measured
/// minimum.
@Suite("QR code size")
struct QRCodeSizeTests {
    /// The shape of a restore code with an S3 key inside: base32 after the
    /// `CVR1-` prefix, 497 bytes in all (version 12, 65 modules).
    private static let restoreCodeWithKey =
        "CVR1-"
        + String(repeating: "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567", count: 16)
        .prefix(492)

    @MainActor
    @Test("a code carrying an S3 key is drawn at least 250 points wide")
    func keyCarryingCodeIsAtLeastTheDocumentedSize() throws {
        let side = try #require(QRCode.side(for: Self.restoreCodeWithKey))
        // 65 modules and a four-module quiet zone on each side.
        #expect(side >= 73 * QRCode.minimumModuleSide - 0.001)
        #expect(side >= 250)
    }

    @MainActor
    @Test("a short pairing code is drawn smaller than one carrying a key")
    func shortCodeStaysSmall() throws {
        let short = try #require(QRCode.side(for: "PAIR-1234-5678"))
        let long = try #require(QRCode.side(for: Self.restoreCodeWithKey))
        #expect(short < long)
        #expect(short < 120)
    }

    @MainActor
    @Test("every module is drawn the minimum module side or more")
    func moduleSideHolds() {
        #expect(QRCode.minimumModuleSide * 105 == 360)
    }
}

extension QRCode {
    /// How wide `text` is drawn, in points, or nil when no code can be made
    /// for it.
    static func side(for text: String) -> CGFloat? {
        image(for: text).map { $0.size.width * minimumModuleSide }
    }
}
