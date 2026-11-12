import BaeKit
import Foundation
import Testing

@testable import bae

/// A line of facts is joined by the catalog's separator, and a missing fact
/// leaves no empty slot.
@Suite("Fact lists")
struct FactListTests {
    @Test("facts are joined by the separator")
    func joinsFacts() {
        let separator = FactList.separator
        #expect(
            FactList.joined(["Vinyl", "1973"])
                == "Vinyl" + separator + "1973"
        )
    }

    @Test("a missing or empty fact leaves no slot")
    func skipsMissingFacts() {
        let line = FactList.joined([nil, "Vinyl", "", "UK"])
        #expect(line == "Vinyl" + FactList.separator + "UK")
        #expect(FactList.joined([nil, ""]).isEmpty)
    }

    @Test("the glyph is the separator without its spaces")
    func glyphHasNoSpaces() {
        #expect(!FactList.glyph.isEmpty)
        #expect(FactList.separator.contains(FactList.glyph))
        #expect(
            FactList.glyph
                == FactList.glyph.trimmingCharacters(in: .whitespaces)
        )
    }

    @Test("a context before a line reads through the catalog's colon message")
    func contextLine() {
        let error = DisplayError(line: "Disk full")
        #expect(
            error.addingContext("Save failed").line == "Save failed: Disk full"
        )
    }

    @Test("a release named by year and media reads year first")
    func describedName() {
        let name = BridgeReleaseName.described(year: 1997, media: [])
        #expect(name.text == "1997")
    }
}
