import Testing

@testable import bae

/// A chip that holds several values writes every one of them.
@Suite("Identifier chip values")
struct IdentifierChipValuesTests {
    @Test("every value is written, separated by a dot")
    func textListsEveryValue() {
        #expect(
            IdentifierChipValues.text([
                "XX0000000001", "XX0000000002", "XX0000000003",
            ]) == "XX0000000001 · XX0000000002 · XX0000000003"
        )
        #expect(IdentifierChipValues.text(["XX0000000001"]) == "XX0000000001")
    }

    @Test("accessibility reads every value")
    func spokenListsEveryValue() {
        let spoken = IdentifierChipValues.spoken([
            "XX0000000001", "XX0000000002", "XX0000000003",
        ])
        for code in ["XX0000000001", "XX0000000002", "XX0000000003"] {
            #expect(spoken.contains(code))
        }
    }
}
