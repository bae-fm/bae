import BaeKit
import Testing

@testable import bae

/// A chip that holds several values shows the one core names and counts the
/// rest behind +N.
@Suite("Identifier chip values")
struct MacIdentifierChipValuesTests {
    private let codes = (1...11).map { String(format: "XX00000000%02d", $0) }

    private var list: MacIdentifierChipValues {
        MacIdentifierChipValues(shown: codes[0], moreCount: 10, all: codes)
    }

    @Test("the button reads the count core hands over")
    func buttonReadsTheCount() {
        #expect(list.moreTitle == "+10")
    }

    @Test("a chip that shows every value has no button")
    func noCountNoButton() {
        let one = MacIdentifierChipValues(
            shown: codes[0],
            moreCount: 0,
            all: [codes[0]]
        )

        #expect(one.moreTitle == nil)
    }

    @Test("accessibility reads every value")
    func spokenListsEveryValue() {
        let spoken = list.spoken
        for code in codes {
            #expect(spoken.contains(code))
        }
    }

    @Test("catalog numbers nothing confirms are folded behind their count")
    func catalogCandidatesFoldBehindTheirCount() {
        let numbers = (1...8).map { "HR-100\($0)" }
        let fold = CatalogCandidateFold(values: numbers, count: 8)

        #expect(fold.shown(expanded: false).isEmpty)
        #expect(fold.buttonTitle(expanded: false) == "+8 more")
        #expect(fold.shown(expanded: true) == numbers)
        #expect(fold.buttonTitle(expanded: true) == "Show fewer")
    }

    @Test("with no catalog number to fold, the band draws no button")
    func noCandidatesNoButton() {
        let fold = CatalogCandidateFold(values: [], count: 0)

        #expect(fold.buttonTitle(expanded: false) == nil)
        #expect(fold.buttonTitle(expanded: true) == nil)
    }

    @Test("the run's fold is what core lists, in its order, with its count")
    func runFoldKeepsCoreOrderAndCount() {
        var run = PreviewData.identifyRunFound
        let numbers = (1...3).map { "HR-100\($0)" }
        run.catalog = .numbers(
            rows: [],
            candidates: numbers.map { BridgeCatalogCandidate(value: $0) }
        )

        #expect(run.catalogCandidateFold.values == numbers)
        #expect(run.catalogCandidateFold.count == 3)
    }
}
