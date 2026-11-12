import BaeKit
import Foundation
import Testing

@testable import bae

/// A new format's number is written in the digits of the app's language: the
/// zero digit the app hands core is one core accepts, and core writes and reads
/// the number in those digits.
@Suite("Save preset numerals")
struct SavePresetNumeralsTests {
    private func rename(zeroDigit: String) throws -> String {
        try bridgeSaveNameAfterFormatChange(
            name: "FLAC (2)",
            from: .flac,
            to: .mp3,
            taken: ["MP3"],
            zeroDigit: zeroDigit
        )
    }

    @Test("the app's own zero digit is one core accepts")
    func theAppsZeroDigitIsAccepted() throws {
        _ = try rename(zeroDigit: Outputs.zeroDigit)
    }

    @Test(
        "a prefilled name follows a format change in the language's digits",
        arguments: [("0", "FLAC (2)", "MP3 (2)"), ("٠", "FLAC (٢)", "MP3 (٢)")]
    )
    func followsInTheLanguagesDigits(
        zeroDigit: String,
        name: String,
        expected: String
    ) throws {
        let renamed = try bridgeSaveNameAfterFormatChange(
            name: name,
            from: .flac,
            to: .mp3,
            taken: ["MP3"],
            zeroDigit: zeroDigit
        )
        #expect(renamed == expected)
    }

    @Test("a zero digit that starts no run of ten digits is an error")
    func anInvalidZeroDigitThrows() {
        #expect(throws: (any Error).self) { try rename(zeroDigit: "a") }
    }
}
