import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

@MainActor
final class FindOnlinePaneTests: XCTestCase {
    /// Nothing to list means nothing to scroll: a folder nobody has looked
    /// up yet offers the Identify button and the collapsed search alone.
    func testAPaneWithNothingToOfferHasNoResultsScroller() async throws {
        let size = NSSize(width: 900, height: 600)
        try await FindOnlineRendering.withHosted(
            MacFindOnline.preview(state: PreviewData.searchStateIdle)
                .importPreviewEnvironment(),
            size: size
        ) { _, host in

            try await SnapshotTestSupport.settle(host)

            XCTAssertFalse(
                SnapshotTestSupport.descendants(of: host)
                    .contains { $0 is NSScrollView }
            )
        }
    }

    /// A failure with no band to hang a capsule's Retry off — a folder that
    /// carried nothing to lay out, or a verdict stored before its signals
    /// were — is a dead end without this: the reasons, and one way to ask
    /// again beneath them. A failure that does have a band says it in the
    /// capsule that failed instead, so the lines come in the one case and
    /// not the other. How they look is the `import-search-failure-lines`
    /// shot's.
    func testAFailureWithNoBandOffersItsRetry() {
        let withoutBand = PreviewData.searchStateFailedWithoutRun
        let withBand = PreviewData.searchStateAllSourcesFailed

        XCTAssertEqual(
            FindOnlineResultArea(identifyState: withoutBand.identifyState),
            .failureLines
        )
        XCTAssertFalse(withoutBand.failureLines.isEmpty)
        XCTAssertTrue(withBand.failureLines.isEmpty)
    }

    /// A sole match whose release lists no tracks says why it was not
    /// picked, under its row, in the words the pane uses beside Import.
    func testASoleMatchThatListsNoTracksSaysWhy() throws {
        let state = PreviewData.searchStateSoleUnchecked

        XCTAssertEqual(
            FindOnlineResultArea(identifyState: state.identifyState),
            .groups
        )
        XCTAssertNil(state.needsYou)
        let note = try XCTUnwrap(state.folderCheck?.localizedText)
        XCTAssertTrue(note.contains("The release lists no tracks"), note)
    }

    /// A folder waiting on the person opens its page with one sentence
    /// saying what happened, and offers to keep its own draft in words that
    /// follow whether releases were offered.
    func testAFolderWaitingOnThePersonSaysWhatHappened() {
        // Signals that named different releases, waiting on the person to
        // pick among them.
        let matches = BridgeNeedsYouReason.matches(count: 2)
        XCTAssertTrue(
            matches.sentence.localizedCaseInsensitiveContains("could be this"),
            matches.sentence
        )
        XCTAssertEqual(
            FindOnlineAutomaticSection.keepOwnDraftTitle(matches),
            String(localized: "None of these")
        )

        // Nothing matched anywhere, waiting on the person.
        let notFound = BridgeNeedsYouReason.notFound
        XCTAssertEqual(
            FindOnlineResultArea(
                identifyState: .notFoundAnywhere(
                    run: PreviewData.identifyRunNothingFound
                )
            ),
            .nothingFound
        )
        XCTAssertEqual(notFound.sentence, "No catalog has this.")
        XCTAssertEqual(
            FindOnlineAutomaticSection.keepOwnDraftTitle(notFound),
            String(localized: "Keep my info")
        )
    }

    /// Which section is open is the session's to say, not the pane's: the
    /// search's form fields are AppKit text fields, there only when the
    /// pane opens on the search.
    func testThePaneOpensOnTheSectionItWasGiven() async throws {
        let onSearch = try await textFields(
            MacFindOnline.preview(
                state: PreviewData.searchStateIdle,
                openSection: .search
            )
        )
        let onAutomatic = try await textFields(
            MacFindOnline.preview(state: PreviewData.searchStateIdle)
        )

        // Artist and Album.
        XCTAssertEqual(onSearch, onAutomatic + 2)
    }

    /// How many AppKit text fields `pane` holds once it settles.
    private func textFields(_ pane: MacFindOnline) async throws -> Int {
        let size = NSSize(width: 900, height: 600)
        return try await FindOnlineRendering.withHosted(
            pane.importPreviewEnvironment().windowBackground(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .filter(\.isEditable)
                .count
        }
    }
}

/// What agreement set aside: the line counting it, and the rows each card
/// shows once it is open. Where the rows sit above the line is the
/// `import-search-narrowed-out-open` shot's.
@MainActor
@Suite("What the signals narrowed out")
struct NarrowedOutDisclosureTests {
    /// The line stands under the matches only when the agreement discarded
    /// something, and it counts the releases behind it.
    @Test(
        "it counts what agreement left out, and says nothing when it left nothing"
    )
    func itCountsWhatAgreementLeftOut() {
        let narrowed = PreviewData.searchStateNarrowedOut.narrowedOutCount
        #expect(narrowed == 3)
        #expect(
            NarrowedOutDisclosure.title(count: narrowed, isExpanded: false)
                == "3 more releases"
        )
        #expect(PreviewData.searchStateFoundExact.narrowedOutCount == 0)
    }

    /// Closed, those releases are only a count. Open, every one of them is on
    /// its album's card — the ones on a card the matches are on and the ones
    /// on a card of their own alike — and the line offers to put them away.
    @Test("opening it lists every release set aside")
    func openingItListsThem() {
        let setAside = ["1871-2", "AAA-001", "BBB-002"]
        let groups = PreviewData.searchStateNarrowedOut.identifiedGroups
        let closed = catalogNumbers(groups, showsNarrowedOut: false)
        let open = catalogNumbers(groups, showsNarrowedOut: true)
        for number in setAside {
            #expect(!closed.contains(number), "\(number)")
            #expect(open.contains(number), "\(number)")
        }
        #expect(
            NarrowedOutDisclosure.title(count: 3, isExpanded: true)
                == "Show fewer"
        )
    }

    /// A row set aside of an album the matches are on shows on that album's
    /// card, behind the same disclosure — not as a second card for the album.
    @Test("a row set aside shows on its album's card once the list is open")
    func aRowSetAsideShowsOnItsAlbumsCard() {
        let group = [PreviewData.searchGroupExactWithSetAside]
        let closed = catalogNumbers(group, showsNarrowedOut: false)
        let open = catalogNumbers(group, showsNarrowedOut: true)
        #expect(!closed.contains("1871-2"))
        #expect(closed.contains("6006-2"))
        #expect(open.contains("1871-2"))
        #expect(open.contains("6006-2"))
    }

    /// The catalog numbers of the rows `groups`' cards show.
    private func catalogNumbers(
        _ groups: [ReleaseGroup],
        showsNarrowedOut: Bool
    ) -> [String] {
        groups.flatMap { group in
            ReleaseGroupSection(
                group: group,
                showsNarrowedOut: showsNarrowedOut,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                onSelect: { _ in }
            )
            .shownSections
            .flatMap(\.rows)
            .flatMap(\.labels)
            .flatMap(\.catalogNumbers)
        }
    }
}

/// The pane's standing notice that Discogs was never asked, above both
/// sections since neither asked it. How it looks is the
/// `import-search-discogs-bar` shot's.
@MainActor
@Suite("The Discogs-not-configured bar")
struct FindOnlineDiscogsBarTests {
    @Test("it stands over the pane until it is put away")
    func itStandsUntilPutAway() {
        let noKey =
            PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: false
            )
            .config
        let key =
            PreviewData.makeConfigStore(
                libraryFullWidth: false,
                discogsUsable: true
            )
            .config
        #expect(MacFindOnline.showsDiscogsBar(noKey, dismissed: false))
        #expect(!MacFindOnline.showsDiscogsBar(noKey, dismissed: true))
        #expect(
            !MacFindOnline.showsDiscogsBar(key, dismissed: false),
            "a library with a token never shows the bar"
        )

        let uiStore = UiStore()
        #expect(!uiStore.discogsNoticeDismissed)
        uiStore.dismissDiscogsNotice()
        #expect(uiStore.discogsNoticeDismissed)
    }
}

@MainActor
@Suite("What picking a pressing row claims")
struct FindOnlinePressingPickTests {
    /// A row is one pressing however many sources carry it, and what picking
    /// it claims is core's answer, not the row's — the pane hands core's pick
    /// straight back through `ImportSearchResultRow.onSelect`.
    @Test("a row sends the pick core settled for it")
    func aRowSendsTheCorePick() throws {
        let bridge = PreviewData.exactPressings[1]
        let pressing = try #require(Pressing(bridge: bridge))

        #expect(pressing.link == bridge.pick)
        let link = pressing.link
        #expect(link.partners.count == 1)
        #expect(
            ([link.record] + link.partners).map(\.key)
                == bridge.releases.map(\.releaseId)
        )
    }

    /// A pressing only one source lists claims only that source.
    @Test("an unpaired row carries no partner")
    func anUnpairedRowCarriesNoPartner() throws {
        let bridge = PreviewData.exactPressings[0]
        let pressing = try #require(Pressing(bridge: bridge))

        #expect(pressing.link == bridge.pick)
        #expect(pressing.link.partners.isEmpty)
        #expect(pressing.link.record.key == bridge.releases[0].releaseId)
    }
}

@MainActor
@Suite("The way into the form")
struct FindOnlineFormFocusTests {
    /// "Search manually" is a request, not a flag: the cursor goes to the
    /// form's first field on every new one, so it works after the person has
    /// clicked elsewhere.
    @Test("each new focus request moves the cursor into the first field")
    func eachRequestMovesTheCursor() async throws {
        let size = NSSize(width: 660, height: 60)
        try await SnapshotTestSupport.withHostedWindow(
            form(focusRequest: 1).frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)

            let artist = try #require(
                SnapshotTestSupport.descendants(of: host)
                    .compactMap { $0 as? NSTextField }
                    .first {
                        $0.placeholderString == String(localized: "Artist")
                    }
            )
            #expect(artist.currentEditor() === window.firstResponder)

            HostedInput.focus(nil, in: window)
            try await SnapshotTestSupport.settle(host)
            #expect(artist.currentEditor() == nil)

            host.rootView = form(focusRequest: 1)
                .frame(width: size.width, height: size.height)
            try await SnapshotTestSupport.settle(host)
            #expect(artist.currentEditor() == nil)

            host.rootView = form(focusRequest: 2)
                .frame(width: size.width, height: size.height)
            try await SnapshotTestSupport.settle(host)
            #expect(artist.currentEditor() === window.firstResponder)
        }
    }

    private func form(focusRequest: Int) -> ImportSearchFormView {
        ImportSearchFormView(
            form: CandidateSearchState(),
            onCommit: { _ in },
            signals: nil,
            focusRequest: focusRequest,
            onSearch: { _ in },
        )
    }
}

@MainActor
@Suite("Find online section glyphs")
struct FindOnlineSectionGlyphTests {
    @Test("identification's glyph follows its state")
    func identificationGlyph() {
        #expect(FindOnlineSectionGlyph(identifyState: .idle) == .none)
        #expect(FindOnlineSectionGlyph(identifyState: .queued) == .queued)
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateTriangulating
                    .identifyState
            ) == .working
        )
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateFoundExact.identifyState
            ) == .none
        )
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateNotFound.identifyState
            ) == .empty
        )
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateNoSignals.identifyState
            ) == .nothing
        )
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateSourceFailure
                    .identifyState
            ) == .failed
        )
    }

    @Test("the search's glyph follows its status, and is nothing before one")
    func searchGlyph() {
        #expect(FindOnlineSectionGlyph(search: nil) == .none)
        #expect(
            FindOnlineSectionGlyph(search: PreviewData.searchRunInFlight)
                == .working
        )
        #expect(
            FindOnlineSectionGlyph(search: PreviewData.manualSearchRun)
                == .none
        )
        #expect(
            FindOnlineSectionGlyph(search: PreviewData.searchRunEmpty) == .empty
        )
        #expect(
            FindOnlineSectionGlyph(search: PreviewData.searchRunSourceFailed)
                == .failed
        )
    }

    /// A collapsed section with nothing to show dims, so the open one beside
    /// it reads as the place to look.
    @Test("only the empty glyphs read as vacant")
    func vacancy() {
        #expect(FindOnlineSectionGlyph.empty.isVacant)
        #expect(FindOnlineSectionGlyph.nothing.isVacant)
        #expect(!FindOnlineSectionGlyph.working.isVacant)
        #expect(!FindOnlineSectionGlyph.failed.isVacant)
        #expect(!FindOnlineSectionGlyph.none.isVacant)
    }
}

@MainActor
@Suite("Find online result area")
struct FindOnlineResultAreaTests {
    @Test("each identify state picks its own area")
    func eachStatePicksItsArea() {
        #expect(FindOnlineResultArea(identifyState: .idle) == .notStarted)
        #expect(FindOnlineResultArea(identifyState: .queued) == .queued)
        #expect(
            FindOnlineResultArea(
                identifyState: .triangulating(
                    run: PreviewData.identifyRunStarting,
                    groups: [],
                    agreements: [:],
                    narrowedOutCount: 0
                )
            ) == .identifying
        )
        #expect(
            FindOnlineResultArea(
                identifyState: PreviewData.searchStateFoundExact.identifyState
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(identifyState: .notFoundAnywhere(run: nil))
                == .nothingFound
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .manualOnly(trackCount: 9, run: nil)
            ) == .noSignals
        )
    }

    /// A run bae broke shows why, not an empty list, and its section reads
    /// as failed.
    @Test("a run bae broke shows its error")
    func aRunBaeBrokeShowsItsError() {
        let failure = BridgeInternalFailure(
            detail: "checking the library: the store is locked"
        )
        #expect(
            FindOnlineResultArea(identifyState: .error(failure: failure))
                == .error(failure)
        )
        #expect(
            FindOnlineSectionGlyph(identifyState: .error(failure: failure))
                == .failed
        )
    }

    /// A folder with nothing to look up on its own but catalog numbers to
    /// offer shows the band's chips rather than the no-signals line.
    @Test("catalog numbers to activate are an area of their own")
    func catalogNumbersToActivate() {
        #expect(
            FindOnlineResultArea(
                identifyState: PreviewData.searchStateAwaitingCatalog
                    .identifyState
            ) == .awaitingCatalog
        )
    }

    /// One source failing never blanks the pane: the other's matches stand,
    /// and only a run that turned up nothing at all shows the reasons.
    @Test("a failure with matches still lists them")
    func aFailureWithMatchesListsThem() {
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateSourceFailure.identifyState
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateAllSourcesFailed.identifyState
            ) == .failureLines
        )
    }
}

@MainActor
@Suite("The pick the pane shows being read")
struct FindOnlineReadingPickTests {
    /// The sole match core picked on its own is the row that holds the
    /// spinner while it is read; with no pick being read none does.
    @Test("a sole match selects itself while core reads it")
    func aSoleMatchSelectsItself() {
        #expect(
            PreviewData.searchStateReadingPick.pickBeingRead == "rel-456"
        )
        #expect(PreviewData.searchStateFoundExact.pickBeingRead == nil)
    }
}

/// How each shape of run draws its band — a number in the run against one
/// waiting, a code left out against one asked about, a Disc ID left out
/// against one no provider answers, a run on one source — is the
/// `identify-band-runs` shot's.
@MainActor
@Suite("The band a run shows")
struct IdentifierBandTests {
    /// A run in flight lists what has landed under its band, so the area
    /// scrolls.
    @Test("the matches landed so far list under the band")
    func landedMatchesListUnderTheBand() async throws {
        let size = NSSize(width: 900, height: 600)
        try await FindOnlineRendering.withHosted(
            MacFindOnline.preview(
                state: PreviewData.searchStateTriangulating
            )
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(
                SnapshotTestSupport.descendants(of: host)
                    .contains { $0 is NSScrollView }
            )
        }
    }
}

/// A pick being read is not yet a pick. Its row says so — no second click to
/// make — and the selected highlight waits until the draft carries the pick.
@MainActor
@Suite("The row a pick is being read on")
struct PickedRowTests {
    @Test("the row being read draws as loading, not as selected")
    func theRowBeingReadIsNotSelected() throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )

        let reading = section(
            loadingPick: LoadingPick(
                releaseId: pressing.lead.releaseId,
                reading: .followed(nil)
            )
        )
        #expect(reading.loading(pressing) != nil)
        #expect(!reading.isSelected(pressing))

        let picked = section(selectedReleaseId: pressing.lead.releaseId)
        #expect(picked.isSelected(pressing))
        #expect(picked.loading(pressing) == nil)
    }

    @Test("the row being read cannot be picked again")
    func theRowBeingReadCannotBePickedAgain() throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )

        let loading = LoadingPick(
            releaseId: pressing.lead.releaseId,
            reading: .followed(nil)
        )
        #expect(!row(pressing, loading: loading).isPickable)
        #expect(row(pressing, loading: nil).isPickable)
    }

    private func section(
        selectedReleaseId: String? = nil,
        loadingPick: LoadingPick? = nil
    ) -> ReleaseGroupSection {
        ReleaseGroupSection(
            group: PreviewData.searchGroupExact,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: selectedReleaseId,
            loadingPick: loadingPick,
            onSelect: { _ in }
        )
    }

    private func row(
        _ pressing: Pressing,
        loading: LoadingPick?
    ) -> ImportSearchResultRow {
        ImportSearchResultRow(
            pressing: pressing,
            isImporting: false,
            libraryStatus: nil,
            isSelected: false,
            loading: loading,
            onSelect: { _ in }
        )
    }
}

/// Hosting a view for the checks of its view tree.
///
/// Hosts without making the window key. Key status is process-wide: a window
/// taking it ends the field editing in whatever window had it, and these tests
/// run alongside ones that type into a field and expect it to still be
/// focused. Reading the view tree needs layout, not focus.
@MainActor
enum FindOnlineRendering {
    /// `view` framed to `size` and hosted for the length of `body`.
    static func withHosted<Value>(
        _ view: some View,
        size: NSSize,
        _ body: (NSWindow, NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            view.frame(width: size.width, height: size.height),
            size: size
        ) {
            try await body($0, $1)
        }
    }
}
