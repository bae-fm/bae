import BaeKit
import Testing

@testable import bae

/// A chip that holds several values writes every one of them.
@Suite("Identifier chip values")
struct MacIdentifierChipValuesTests {
    @Test("every value is written, separated by a dot")
    func textListsEveryValue() {
        #expect(
            MacIdentifierChipValues.text([
                "XX0000000001", "XX0000000002", "XX0000000003",
            ]) == "XX0000000001 · XX0000000002 · XX0000000003"
        )
        #expect(
            MacIdentifierChipValues.text(["XX0000000001"]) == "XX0000000001"
        )
    }

    @Test("accessibility reads every value")
    func spokenListsEveryValue() {
        let spoken = MacIdentifierChipValues.spoken([
            "XX0000000001", "XX0000000002", "XX0000000003",
        ])
        for code in ["XX0000000001", "XX0000000002", "XX0000000003"] {
            #expect(spoken.contains(code))
        }
    }

    @Test("catalog numbers nothing confirms are folded behind their count")
    func catalogCandidatesFoldBehindTheirCount() {
        let numbers = (1...8).map { "HR-100\($0)" }
        let fold = CatalogCandidateFold(values: numbers)

        #expect(fold.shown(expanded: false).isEmpty)
        #expect(fold.buttonTitle(expanded: false) == "+8 more")
        #expect(fold.shown(expanded: true) == numbers)
        #expect(fold.buttonTitle(expanded: true) == "Show fewer")
    }

    @Test("with no catalog number to fold, the band draws no button")
    func noCandidatesNoButton() {
        let fold = CatalogCandidateFold(values: [])

        #expect(fold.buttonTitle(expanded: false) == nil)
        #expect(fold.buttonTitle(expanded: true) == nil)
    }

    @Test("the run's candidates are the numbers core lists, in its order")
    func runCandidatesKeepCoreOrder() {
        var run = PreviewData.identifyRunFound
        let numbers = (1...3).map { "HR-100\($0)" }
        run.catalog = .numbers(
            rows: [],
            candidates: numbers.map { BridgeCatalogCandidate(value: $0) }
        )

        #expect(run.catalogCandidateValues == numbers)
    }
}
