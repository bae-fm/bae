import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import Vision
import XCTest

@testable import bae

@MainActor
final class FindOnlinePaneTests: XCTestCase {
    /// Nothing to list means nothing to scroll: a folder nobody has looked
    /// up yet offers the Identify button and the collapsed search alone.
    func testAPaneWithNothingToOfferHasNoResultsScroller() async {
        let size = NSSize(width: 900, height: 600)
        let (window, host) = FindOnlineRendering.host(
            ImportSearchPane.preview(state: PreviewData.searchStateIdle)
                .importPreviewEnvironment(),
            size: size
        )

        await Task.yield()
        host.layoutSubtreeIfNeeded()

        XCTAssertFalse(
            SnapshotTestSupport.descendants(of: host)
                .contains { $0 is NSScrollView }
        )
        withExtendedLifetime(window) {}
    }

    /// A failure with no band to hang a capsule's Retry off — a folder that
    /// carried nothing to lay out, or a verdict stored before its signals
    /// were — is a dead end without this: the reasons, and one way to ask
    /// again beneath them. A failure that does have a band says it in the
    /// capsule that failed instead, so the word appears in the one case and
    /// not the other.
    ///
    /// Read off the rendered pane: the pane draws its own controls rather than
    /// hanging AppKit ones in the view tree, so what it says is in its pixels.
    func testAFailureWithNoBandOffersItsRetry() async throws {
        let retry = String(localized: "Retry")
        let withoutBand = try await renderedText(
            of: PreviewData.searchStateFailedWithoutRun
        )
        let withBand = try await renderedText(
            of: PreviewData.searchStateAllSourcesFailed
        )

        XCTAssertTrue(
            withoutBand.contains {
                $0.localizedCaseInsensitiveContains(retry)
            },
            "a failure with no band reads: \(withoutBand)"
        )
        XCTAssertFalse(
            withBand.contains { $0.localizedCaseInsensitiveContains(retry) },
            "a failure with a band reads: \(withBand)"
        )
    }

    /// Nothing has run for this candidate, and starting one is the card's
    /// action, not the pane's: the not-started area offers the other way to a
    /// release — asking for it by name — and no way to start a run.
    func testANotStartedPaneOffersOnlyTheTypedSearch() async throws {
        let lines = try await renderedText(of: PreviewData.searchStateIdle)

        XCTAssertTrue(
            lines.contains {
                $0.localizedCaseInsensitiveContains(
                    String(localized: "Search manually")
                )
            },
            "a not-started pane reads: \(lines)"
        )
        XCTAssertFalse(
            lines.contains {
                $0.trimmingCharacters(in: .whitespacesAndNewlines)
                    .caseInsensitiveCompare("Identify") == .orderedSame
            },
            "a not-started pane reads: \(lines)"
        )
    }

    /// Which section the pane opens on is the entry's to say, not the pane's:
    /// the entry that asked for a run opens on the run, the one that asked to
    /// search by name opens on the form.
    func testThePaneOpensOnTheSectionItWasGiven() async throws {
        let onSearch = try await FindOnlineRendering.text(
            ImportSearchPane.preview(
                state: PreviewData.searchStateIdle,
                initialSection: .search
            )
            .importPreviewEnvironment(),
            size: NSSize(width: 900, height: 600)
        )
        let onAutomatic = try await renderedText(
            of: PreviewData.searchStateIdle
        )

        // The form's field placeholders, not its tab labels: recognition on
        // a slow runner has read "Catalog #" as "Cataloon", while a plain
        // word survives.
        let formFields = [
            String(localized: "Artist"), String(localized: "Album"),
        ]
        for formField in formFields {
            XCTAssertTrue(
                onSearch.contains {
                    $0.localizedCaseInsensitiveContains(formField)
                },
                "a pane opened on the search reads: \(onSearch)"
            )
            XCTAssertFalse(
                onAutomatic.contains {
                    $0.localizedCaseInsensitiveContains(formField)
                },
                "a pane opened on the run reads: \(onAutomatic)"
            )
        }
    }

    /// Every line of text the pane draws for `state`.
    private func renderedText(
        of state: ImportSearchState
    ) async throws -> [String] {
        try await FindOnlineRendering.text(
            ImportSearchPane.preview(state: state).importPreviewEnvironment(),
            size: NSSize(width: 900, height: 600)
        )
    }
}

@MainActor
@Suite("What the signals narrowed out")
struct NarrowedOutDisclosureTests {
    private static let paneSize = NSSize(width: 900, height: 600)
    private static let disclosureSize = NSSize(width: 660, height: 420)

    /// The line stands under the matches only when the agreement discarded
    /// something, and it counts the releases behind it.
    @Test(
        "it counts what agreement left out, and says nothing when it left nothing"
    )
    func itCountsWhatAgreementLeftOut() async throws {
        let narrowed = try await FindOnlineRendering.text(
            ImportSearchPane.preview(state: PreviewData.searchStateNarrowedOut)
                .importPreviewEnvironment(),
            size: Self.paneSize
        )
        #expect(narrowed.contains { $0.contains("2 more releases") })

        let agreed = try await FindOnlineRendering.text(
            ImportSearchPane.preview(state: PreviewData.searchStateFoundExact)
                .importPreviewEnvironment(),
            size: Self.paneSize
        )
        #expect(
            !agreed.contains {
                $0.localizedCaseInsensitiveContains("more releases")
            }
        )
    }

    /// Closed, those releases are only a count; open, they are cards to pick
    /// like any other.
    @Test("opening it lists the releases the agreement discarded")
    func openingItListsThem() async throws {
        let closed = try await FindOnlineRendering.text(
            disclosure(isExpanded: false),
            size: Self.disclosureSize
        )
        let open = try await FindOnlineRendering.text(
            disclosure(isExpanded: true),
            size: Self.disclosureSize
        )
        #expect(!closed.contains { $0.contains("Other Album Title") })
        #expect(open.contains { $0.contains("Other Album Title") })
    }

    private func disclosure(isExpanded: Bool) -> some View {
        NarrowedOutDisclosure(
            narrowedOut: PreviewData.searchStateNarrowedOut.narrowedOut,
            isExpanded: .constant(isExpanded),
            isImporting: false,
            selectedReleaseId: nil,
            loadingReleaseId: nil,
            onSelect: { _ in },
        )
        .importPreviewEnvironment()
    }
}

/// The pane's standing notice that Discogs was never asked.
///
/// Asserted in pixels: the bar sits above both section headers, so what it
/// changes is the whole pane, and putting it away takes that row with it.
///
/// What dismissal does *not* take away is the header's own report: Discogs's
/// checkbox stays unchecked and cannot be moved for as long as there is no
/// token. So a dismissed pane no longer matches a configured one — the notice
/// is dismissible, the fact it reports is not.
@MainActor
@Suite("The Discogs-not-configured bar")
struct FindOnlineDiscogsBarTests {
    private static let size = NSSize(width: 900, height: 620)

    /// AUTOMATIC open, with nothing looked up yet.
    @Test("it stands over the automatic section until it is put away")
    func overTheAutomaticSection() async throws {
        try await assertBarIsTheDifference(state: PreviewData.searchStateIdle)
    }

    /// SEARCH open. The per-source lines no longer say anything about a
    /// source that was never asked, so this bar is all there is to say it.
    @Test("and over the search section, which no longer says it per source")
    func overTheSearchSection() async throws {
        try await assertBarIsTheDifference(
            state: PreviewData.searchStateSearchEmpty
        )
    }

    private func assertBarIsTheDifference(
        state: ImportSearchState
    ) async throws {
        let showing = try await pixels(
            state: state,
            discogsUsable: false,
            dismissed: false
        )
        let dismissed = try await pixels(
            state: state,
            discogsUsable: false,
            dismissed: true
        )
        let configured = try await pixels(
            state: state,
            discogsUsable: true,
            dismissed: false
        )

        #expect(showing != dismissed, "putting the bar away takes its row")
        #expect(
            showing != configured,
            "a library with a token never shows the bar"
        )
        #expect(
            dismissed != configured,
            "the header still says Discogs is not being asked"
        )
    }

    /// And that residual difference is the header: the same two libraries, with
    /// no bar in either, still draw their Discogs checkbox differently.
    @Test("the header keeps saying it after the notice is gone")
    func theHeaderKeepsSayingIt() async throws {
        let noToken = try await FindOnlineRendering.pixels(
            header(discogsUsable: false),
            size: NSSize(width: 900, height: 42)
        )
        let configured = try await FindOnlineRendering.pixels(
            header(discogsUsable: true),
            size: NSSize(width: 900, height: 42)
        )

        #expect(noToken != configured)
    }

    private func header(discogsUsable: Bool) -> some View {
        FindOnlineHeader(onBack: {})
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    discogsUsable: discogsUsable
                )
            )
            .environment(Importer.stub())
            .environment(UiStore())
    }

    private func pixels(
        state: ImportSearchState,
        discogsUsable: Bool,
        dismissed: Bool
    ) async throws -> Data {
        let uiStore = UiStore()
        if dismissed {
            uiStore.dismissDiscogsNotice()
        }
        return try await FindOnlineRendering.pixels(
            ImportSearchPane.preview(state: state)
                .environment(uiStore)
                .environment(
                    PreviewData.makeConfigStore(
                        libraryFullWidth: false,
                        discogsUsable: discogsUsable
                    )
                )
                .importPreviewEnvironment(),
            size: Self.size
        )
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

        #expect(pressing.provenance == bridge.pick)
        guard
            case .externalRelease(let record, let partners) =
                pressing.provenance
        else {
            Issue.record("a picked row claims an external release")
            return
        }
        #expect(partners.count == 1)
        #expect(
            ([record] + partners).map(\.key)
                == bridge.releases.map(\.releaseId)
        )
    }

    /// The re-identify footer commits the same claim, only in the shape a
    /// library release takes.
    @Test("the reseed says the same thing as the pick")
    func theReseedSaysTheSameThing() throws {
        let bridge = PreviewData.exactPressings[1]
        let pressing = try #require(Pressing(bridge: bridge))

        guard
            case .externalRelease(let record, let partners) =
                pressing.provenance
        else {
            Issue.record("a picked row claims an external release")
            return
        }
        #expect(
            pressing.reseed
                == .externalRelease(
                    releaseId: record.key,
                    source: record.catalog,
                    partners: partners
                )
        )
    }

    /// A pressing only one source lists claims only that source.
    @Test("an unpaired row carries no partner")
    func anUnpairedRowCarriesNoPartner() throws {
        let bridge = PreviewData.exactPressings[0]
        let pressing = try #require(Pressing(bridge: bridge))

        #expect(pressing.provenance == bridge.pick)
        guard
            case .externalRelease(let record, let partners) =
                pressing.provenance
        else {
            Issue.record("a picked row claims an external release")
            return
        }
        #expect(partners.isEmpty)
        #expect(record.key == bridge.releases[0].releaseId)
    }
}

@MainActor
@Suite("The sources a row names")
struct FindOnlinePressingSourceTests {
    /// Which record a row is read from decides what the row shows, not the
    /// order its tags read in: those follow the one order every surface names
    /// sources in, so a row and the card above it never disagree.
    @Test("a row read from the Discogs record still names MusicBrainz first")
    func aDiscogsLedRowNamesMusicBrainzFirst() throws {
        let paired = PreviewData.exactPressings[1]
        let discogsLead = try #require(paired.releases.last)
        let musicBrainzPartner = try #require(paired.releases.first)
        let pressing = try #require(
            Pressing(
                bridge: BridgePressing(
                    releases: [discogsLead, musicBrainzPartner],
                    pick: .externalRelease(
                        record: BridgeMetadataRef(
                            catalog: discogsLead.source,
                            key: discogsLead.releaseId
                        ),
                        partners: [
                            BridgeMetadataRef(
                                catalog: musicBrainzPartner.source,
                                key: musicBrainzPartner.releaseId
                            )
                        ]
                    )
                )
            )
        )

        #expect(pressing.lead.source == .discogs)
        #expect(pressing.sources == [.musicBrainz, .discogs])
    }

    /// A pressing one source lists names that source and nothing else.
    @Test("an unpaired row names the one source that lists it")
    func anUnpairedRowNamesOneSource() throws {
        let pressing = try #require(
            Pressing(bridge: PreviewData.exactPressings[0])
        )

        #expect(pressing.sources == [.musicBrainz])
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
        let (window, host) = SnapshotTestSupport.hostInWindow(
            form(focusRequest: 1).frame(width: size.width, height: size.height),
            size: size
        )
        await SnapshotTestSupport.settle(host)

        let artist = try #require(
            SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
                .first { $0.placeholderString == String(localized: "Artist") }
        )
        #expect(artist.currentEditor() === window.firstResponder)

        _ = window.makeFirstResponder(nil)
        await SnapshotTestSupport.settle(host)
        #expect(artist.currentEditor() == nil)

        host.rootView = form(focusRequest: 1)
            .frame(width: size.width, height: size.height)
        await SnapshotTestSupport.settle(host)
        #expect(artist.currentEditor() == nil)

        host.rootView = form(focusRequest: 2)
            .frame(width: size.width, height: size.height)
        await SnapshotTestSupport.settle(host)
        #expect(artist.currentEditor() === window.firstResponder)
        withExtendedLifetime(window) {}
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
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateTriangulating
                    .identifyState
            ) == .working
        )
        #expect(
            FindOnlineSectionGlyph(
                identifyState: PreviewData.searchStateFoundExact.identifyState
            ) == .matched
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
                == .matched
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
        #expect(!FindOnlineSectionGlyph.matched.isVacant)
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
        #expect(
            FindOnlineResultArea(
                identifyState: .triangulating(
                    run: PreviewData.identifyRunStarting,
                    groups: [],
                    libraryStatuses: [:],
                    agreements: [:],
                    narrowedOut: .nothing
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
@Suite("What the pane finalizes")
struct FindOnlineFinalizingTests {
    /// A sole match core is picking on its own is the row that holds the
    /// spinner; several matches wait on a person and none does.
    @Test("a sole match selects itself while core finalizes")
    func aSoleMatchSelectsItself() {
        #expect(
            PreviewData.searchStateFinalizing.finalizingPressing?.lead.releaseId
                == "rel-456"
        )
        #expect(PreviewData.searchStateFoundExact.finalizingPressing == nil)
    }
}

@MainActor
@Suite("The band a run shows")
struct IdentifierBandTests {
    /// The chips draw themselves — labels, values, provider capsules — rather
    /// than handing anything to an AppKit control. Assert each shape of run
    /// puts something on screen, and that different runs draw differently.
    @Test("every shape of run draws")
    func everyShapeOfRunDraws() async throws {
        let starting = try await Self.pixels(PreviewData.identifyRunStarting)
        let inFlight = try await Self.pixels(PreviewData.identifyRunInFlight)
        let failed = try await Self.pixels(
            PreviewData.identifyRunProviderFailed
        )
        let empty = try await Self.pixels(PreviewData.identifyRunNothingFound)

        #expect(starting != inFlight)
        #expect(inFlight != failed)
        #expect(failed != empty)
    }

    /// A run in flight lists what has landed under its band, so the area
    /// scrolls.
    @Test("the matches landed so far list under the band")
    func landedMatchesListUnderTheBand() async {
        let size = NSSize(width: 900, height: 600)
        let (window, host) = FindOnlineRendering.host(
            ImportSearchPane.preview(
                state: PreviewData.searchStateTriangulating
            )
            .importPreviewEnvironment(),
            size: size
        )
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        #expect(
            SnapshotTestSupport.descendants(of: host)
                .contains { $0 is NSScrollView }
        )
        withExtendedLifetime(window) {}
    }

    /// A catalog number is drawn where it stands: filled, with a capsule per
    /// provider, once it is in the run; outlined and dimmed while it waits.
    /// The same number in the two states draws differently.
    @Test("a catalog number in the run draws differently from one waiting")
    func aNumberInTheRunDrawsDifferently() async throws {
        let active = try await Self.pixels(
            PreviewData.identifyRunProviderFailed
        )
        let waiting = try await Self.pixels(
            PreviewData.identifyRunCatalogWaiting
        )

        #expect(active != waiting)
    }

    /// An identifier the person left out of the run is the off chip: outlined,
    /// dimmed, and with no provider capsules. The same two codes with both
    /// asked about draw differently.
    @Test(
        "a barcode left out of the run draws differently from one asked about"
    )
    func aLeftOutBarcodeDrawsDifferently() async throws {
        let leftOut = try await Self.pixels(
            PreviewData.identifyRunBarcodeLeftOut
        )
        let asked = try await Self.pixels(
            PreviewData.identifyRunBothBarcodesAsked
        )

        #expect(leftOut != asked)
    }

    /// A disc ID nothing looked up is two different situations, and the band
    /// draws them apart: the person took it out, which they can undo, or no
    /// provider the run asks answers disc IDs, which they cannot.
    @Test("a disc ID left out draws differently from one with no provider")
    func aLeftOutDiscIdDrawsDifferently() async throws {
        let leftOut = try await Self.pixels(
            PreviewData.identifyRunDiscIdLeftOut
        )
        let noProvider = try await Self.pixels(
            PreviewData.identifyRunOneSource
        )

        #expect(leftOut != noProvider)
    }

    /// The band on its own, at the width the pane's previews give it.
    private static func pixels(_ run: BridgeIdentifyRun) async throws -> Data {
        try await FindOnlineRendering.pixels(
            IdentifierBand(
                run: run,
                catalogAgreements: [],
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
            )
            .importPreviewEnvironment(),
            size: NSSize(width: 660, height: 260)
        )
    }
}

/// Every chip in the band turns its own identifier over, and what goes back is
/// the whole value of what this candidate's identification asks about — so one
/// chip's click must leave every other decision where it was.
@MainActor
@Suite("Turning one identifier off and on")
struct LookupToggleTests {
    private static let choices = BridgeLookupChoices(
        discIdExcluded: false,
        excludedBarcodes: ["9999999999999"],
        chosenCatalogs: ["LBL 001"],
        discountedCatalogs: ["LBL 100"]
    )

    @Test("the disc ID goes out and comes back, leaving the rest alone")
    func theDiscIdFlips() {
        let out = Self.choices.toggling(.discId)
        #expect(out.discIdExcluded)
        #expect(out.excludedBarcodes == ["9999999999999"])
        #expect(out.chosenCatalogs == ["LBL 001"])
        #expect(out.discountedCatalogs == ["LBL 100"])

        #expect(!out.toggling(.discId).discIdExcluded)
    }

    @Test("one barcode joins the codes left out and the others stay")
    func oneBarcodeGoesOut() {
        let out = Self.choices.toggling(.barcode("0123456789012"))
        #expect(out.excludedBarcodes == ["0123456789012", "9999999999999"])
        #expect(!out.discIdExcluded)
        #expect(out.chosenCatalogs == ["LBL 001"])
    }

    @Test("asking about one code again leaves the other left out")
    func oneBarcodeComesBack() {
        let back = Self.choices.toggling(.barcode("9999999999999"))
        #expect(back.excludedBarcodes.isEmpty)

        let both = Self.choices
            .toggling(.barcode("0123456789012"))
            .toggling(.barcode("9999999999999"))
        #expect(both.excludedBarcodes == ["0123456789012"])
    }

    @Test("a catalog number joins and leaves the numbers looked up")
    func oneCatalogNumberFlips() {
        let added = Self.choices.toggling(.catalog("LBL 002"))
        #expect(added.chosenCatalogs == ["LBL 001", "LBL 002"])
        #expect(added.excludedBarcodes == ["9999999999999"])

        #expect(
            added.toggling(.catalog("LBL 001")).chosenCatalogs == ["LBL 002"]
        )
    }

    /// A struck-out number is not one the run looks up: striking it out
    /// takes it out of the chosen numbers, whatever else stands.
    @Test("striking a number out takes it out of the numbers looked up")
    func strikingOutUnchooses() {
        let struck = Self.choices.discounting("LBL 001", pickedNumber: nil)
        #expect(struck.discountedCatalogs == ["LBL 001", "LBL 100"])
        #expect(struck.chosenCatalogs.isEmpty)
        #expect(struck.excludedBarcodes == ["9999999999999"])
        #expect(!struck.discIdExcluded)
    }

    /// Counting a number again chooses it only when it is the picked
    /// record's own: keeping that agreement is what chose it.
    @Test("counting a number again re-chooses it only for the picked record")
    func countingAgainRechoosesThePickedNumber() {
        let picked = Self.choices.discounting(
            "LBL 100",
            pickedNumber: "LBL 100"
        )
        #expect(picked.discountedCatalogs.isEmpty)
        #expect(picked.chosenCatalogs == ["LBL 001", "LBL 100"])

        let other = Self.choices.discounting("LBL 100", pickedNumber: "LBL 200")
        #expect(other.discountedCatalogs.isEmpty)
        #expect(other.chosenCatalogs == ["LBL 001"])

        #expect(
            Self.choices.discounting("LBL 100", pickedNumber: nil)
                .chosenCatalogs
                == ["LBL 001"]
        )
    }
}

/// A pick being read is not yet a pick. Its row says so — a spinner, and no
/// second click to make — and the selected highlight waits until the draft
/// carries the pick.
@MainActor
@Suite("The row a pick is being read on")
struct PickedRowTests {
    @Test("the row being read draws as loading, not as selected")
    func theRowBeingReadIsNotSelected() throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )

        let reading = section(loadingReleaseId: pressing.lead.releaseId)
        #expect(reading.isLoading(pressing))
        #expect(!reading.isSelected(pressing))

        let picked = section(selectedReleaseId: pressing.lead.releaseId)
        #expect(picked.isSelected(pressing))
        #expect(!picked.isLoading(pressing))
    }

    @Test("the row being read cannot be picked again")
    func theRowBeingReadCannotBePickedAgain() throws {
        let pressing = try #require(
            PreviewData.searchGroupExact.pressings.first
        )

        #expect(!row(pressing, isLoading: true).isPickable)
        #expect(row(pressing, isLoading: false).isPickable)
    }

    private func section(
        selectedReleaseId: String? = nil,
        loadingReleaseId: String? = nil
    ) -> ReleaseGroupSection {
        ReleaseGroupSection(
            group: PreviewData.searchGroupExact,
            isImporting: false,
            libraryStatuses: [:],
            selectedReleaseId: selectedReleaseId,
            loadingReleaseId: loadingReleaseId,
            onSelect: { _ in }
        )
    }

    private func row(
        _ pressing: Pressing,
        isLoading: Bool
    ) -> ImportSearchResultRow {
        ImportSearchResultRow(
            pressing: pressing,
            isImporting: false,
            libraryStatus: nil,
            isSelected: false,
            isLoading: isLoading,
            onSelect: { _ in }
        )
    }
}

/// Rendering a view to pixels, for the checks that a surface drew at all.
///
/// Hosts without making the window key. Key status is process-wide: a window
/// taking it ends the field editing in whatever window had it, and these tests
/// run alongside ones that type into a field and expect it to still be
/// focused. Capture needs layout and `cacheDisplay`, not focus.
@MainActor
enum FindOnlineRendering {
    /// Every line of text a view draws, read off its pixels: these panes draw
    /// their own controls rather than hanging AppKit ones in the view tree, so
    /// what they say is in what they drew.
    /// Read off the window's own surface: text captured against
    /// transparency loses all but its coloured parts, which is a header read
    /// back as its blue link and nothing else.
    static func text(_ view: some View, size: NSSize) async throws -> [String] {
        let (window, host) = host(view.windowBackground(), size: size)
        defer { withExtendedLifetime(window) {} }
        await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(host, size: size)
        return try await SnapshotTestSupport.recognizedText(in: png).map(\.text)
    }

    static func pixels(
        _ view: some View,
        size: NSSize = NSSize(width: 380, height: 220)
    ) async throws -> Data {
        let (window, host) = host(view.windowBackground(), size: size)
        let pixels = try await SnapshotTestSupport.capturePNG(host, size: size)
        withExtendedLifetime(window) {}
        return pixels
    }

    static func host<V: View>(
        _ view: V,
        size: NSSize
    ) -> (window: NSWindow, host: NSHostingView<some View>) {
        let bounds = NSRect(origin: .zero, size: size)
        let host = NSHostingView(
            rootView: view.frame(width: size.width, height: size.height)
        )
        host.frame = bounds
        let window = NSWindow(
            contentRect: bounds,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = host
        return (window, host)
    }
}
