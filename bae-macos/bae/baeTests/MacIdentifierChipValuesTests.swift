import BaeKit
import Testing

@testable import bae

/// A chip that holds several values shows the first and counts the rest.
@Suite("Identifier chip values")
struct MacIdentifierChipValuesTests {
    private let codes = (1...11).map { String(format: "XX00000000%02d", $0) }

    @Test("the chip shows the first value and the count of the rest")
    func firstValueThenTheCountOfTheRest() throws {
        let list = try #require(MacIdentifierChipValues(codes))

        #expect(list.first == "XX0000000001")
        #expect(list.moreTitle == "+10")
    }

    @Test("a chip with no value has no list")
    func noValuesNoList() {
        #expect(MacIdentifierChipValues([]) == nil)
    }

    @Test("one value has nothing behind a count")
    func oneValueHasNoCount() throws {
        let list = try #require(MacIdentifierChipValues(["XX0000000001"]))

        #expect(list.first == "XX0000000001")
        #expect(list.moreTitle == nil)
    }

    @Test("the popover lists every value, one a line, none the first alone")
    func popoverListsEveryValue() {
        #expect(MacIdentifierChipValues(codes)?.lines == codes)
    }

    @Test("accessibility reads every value")
    func spokenListsEveryValue() {
        let spoken = MacIdentifierChipValues(codes)?.spoken ?? ""
        for code in codes {
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
