import AppKit
import BaeKit
import Foundation
import SwiftUI
import Testing
import XCTest

@testable import bae

@MainActor
final class FindOnlinePaneTests: XCTestCase {
    /// Nothing to list means nothing to scroll: the docked form is the whole
    /// of what a folder nobody has looked up yet offers.
    func testAPaneWithNothingToOfferHasNoResultsScroller() async {
        let size = NSSize(width: 900, height: 600)
        let (window, host) = FindOnlineRendering.host(
            ImportSearchPane.preview(state: PreviewData.searchStateIdle),
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
        #expect(pressing.provenance.releaseRefs.count == 2)
        #expect(
            pressing.provenance.releaseRefs.map(\.releaseId)
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
            case .externalRelease(let source, let releaseId, let partners) =
                pressing.provenance
        else {
            Issue.record("a picked row claims an external release")
            return
        }
        #expect(
            pressing.reseed
                == .externalRelease(
                    releaseId: releaseId,
                    source: source,
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
        #expect(
            pressing.provenance.releaseRefs.map(\.releaseId)
                == [bridge.releases[0].releaseId]
        )
    }
}

@MainActor
@Suite("The empty zone's way into the form")
struct FindOnlineFormFocusTests {
    /// "Search instead" is a request, not a flag: the cursor goes to the
    /// form's first field on every new one, so it works after the automatic
    /// hand-over already happened and the person has clicked elsewhere.
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
            activeTab: .constant(.general),
            searchArtist: .constant(""),
            searchAlbum: .constant(""),
            searchCatalog: .constant(""),
            searchBarcode: .constant(""),
            signals: nil,
            focusRequest: focusRequest,
            onSearch: {},
        )
    }
}

@MainActor
@Suite("Find online verdict")
struct FindOnlineVerdictTests {
    @Test("a folder nobody looked up offers to identify it")
    func idleOffersIdentify() {
        let verdict = FindOnlineVerdict(
            state: .idle,
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.lines == [String(localized: "Not identified")])
        #expect(verdict.action == .identify)
        #expect(!verdict.isWorking)
    }

    @Test("a run under way says so and offers nothing")
    func triangulatingWorks() {
        let verdict = FindOnlineVerdict(
            state: .triangulating(discid: .lookingUp, barcode: .scanning),
            toolbar: PreviewData.toolbarBothRunning
        )

        #expect(verdict.isWorking)
        #expect(verdict.action == .none)
    }

    @Test("a verdict names the signals that matched, and only those")
    func foundNamesTheMatchedSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: PreviewData.toolbarBothMatched
        )

        #expect(verdict.action == .adjust)
        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == true)
    }

    @Test("an excluded signal is not part of what identified the folder")
    func foundSkipsExcludedSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: PreviewData.toolbarBarcodeExcluded
        )

        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == false)
    }

    @Test("a verdict stood back up from the store names no signals")
    func resumedVerdictNamesNoSignals() {
        let verdict = FindOnlineVerdict(
            state: PreviewData.searchStateFoundExact.identifyState,
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.lines == [String(localized: "Identified")])
        #expect(verdict.action == .adjust)
    }

    @Test("nothing found names the signals that ran")
    func notFoundNamesTheSignalsThatRan() {
        let verdict = FindOnlineVerdict(
            state: .notFoundAnywhere,
            toolbar: PreviewData.toolbarNothingMatched
        )

        let line = try? #require(verdict.lines.first)
        #expect(line?.contains(String(localized: "Disc ID")) == true)
        #expect(line?.contains(String(localized: "barcode")) == true)
        #expect(verdict.action == .adjust)
    }

    @Test("a folder with no signals has nothing to adjust")
    func manualOnlyOffersNothing() {
        let verdict = FindOnlineVerdict(
            state: .manualOnly(trackCount: 9),
            toolbar: PreviewData.toolbarSkippedNoSignals
        )

        #expect(
            verdict.lines == [String(localized: "No signals in this folder")]
        )
        #expect(verdict.action == .none)
    }

    @Test("a failure names each step, its source and a brief reason")
    func failureNamesEachStep() {
        let verdict = FindOnlineVerdict(
            state: .failed(
                failures: [
                    .discId(failure: .network),
                    .barcode(source: .discogs, failure: .timeout),
                ],
                groups: [],
                libraryStatuses: [:],
                provenance: [:]
            ),
            toolbar: BridgeSignalsToolbar(signals: [])
        )

        #expect(verdict.isFailure)
        #expect(verdict.action == .retry)
        #expect(verdict.lines.count == 2)
        // The disc-ID endpoint is MusicBrainz's alone, so its failure names
        // the source the same way a barcode lookup names its provider.
        #expect(
            verdict.lines[0]
                .contains(bridgeMetadataSourceName(source: .musicBrainz))
        )
        #expect(
            verdict.lines[0]
                .contains(SignalBadgeStyle.sentenceLabel(for: .discId))
        )
        #expect(
            verdict.lines[0].contains(BridgeLookupFailure.network.briefLine)
        )
        #expect(
            verdict.lines[1]
                .contains(bridgeMetadataSourceName(source: .discogs))
        )
        #expect(
            verdict.lines[1]
                .contains(SignalBadgeStyle.sentenceLabel(for: .barcode))
        )
        #expect(
            verdict.lines[1].contains(BridgeLookupFailure.timeout.briefLine)
        )
        #expect(verdict.help.contains(BridgeLookupFailure.timeout.badgeLine))
    }
}

@MainActor
@Suite("Find online result area")
struct FindOnlineResultAreaTests {
    @Test("a submitted search owns the area whatever identification said")
    func aSearchOwnsTheArea() {
        let found = PreviewData.searchStateFoundExact.identifyState
        #expect(
            FindOnlineResultArea(identifyState: found, hasSearch: true)
                == .searchRun
        )
        #expect(
            FindOnlineResultArea(identifyState: .idle, hasSearch: true)
                == .searchRun
        )
    }

    @Test("each identify state picks its own area")
    func eachStatePicksItsArea() {
        #expect(
            FindOnlineResultArea(identifyState: .idle, hasSearch: false)
                == .notStarted
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .triangulating(
                    discid: .computing,
                    barcode: .scanning
                ),
                hasSearch: false
            ) == .identifying
        )
        #expect(
            FindOnlineResultArea(
                identifyState: PreviewData.searchStateFoundExact.identifyState,
                hasSearch: false
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .notFoundAnywhere,
                hasSearch: false
            ) == .nothingFound
        )
        #expect(
            FindOnlineResultArea(
                identifyState: .manualOnly(trackCount: 9),
                hasSearch: false
            ) == .noSignals
        )
    }

    /// One source failing never blanks the pane: the other's matches stand,
    /// and only a run that turned up nothing at all shows the reasons.
    @Test("a failure with matches still lists them")
    func aFailureWithMatchesListsThem() {
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateSourceFailure.identifyState,
                hasSearch: false
            ) == .groups
        )
        #expect(
            FindOnlineResultArea(
                identifyState:
                    PreviewData.searchStateAllSourcesFailed.identifyState,
                hasSearch: false
            ) == .failureLines
        )
    }
}

@MainActor
@Suite("The signal chips a run shows")
struct IdentifyingSignalChipsTests {
    /// The chips are the run's progress, so they go when the run does: a
    /// settled verdict says the same thing in one header line.
    @Test("the chips show only while a run is going")
    func chipsShowOnlyWhileIdentifying() {
        let toolbar = PreviewData.toolbarIdentifying
        #expect(
            FindOnlineResultArea.identifying.showsSignalChips(toolbar: toolbar)
        )
        for area in [
            FindOnlineResultArea.groups,
            .nothingFound,
            .noSignals,
            .failureLines,
            .notStarted,
            .searchRun,
        ] {
            #expect(!area.showsSignalChips(toolbar: toolbar))
        }
    }

    /// A verdict resumed from the store has no signals, so the row would be an
    /// empty strip above the results.
    @Test("a run with no signals to show spends no row on them")
    func noChipsWithoutSignals() {
        #expect(
            !FindOnlineResultArea.identifying.showsSignalChips(
                toolbar: BridgeSignalsToolbar(signals: [])
            )
        )
    }

    /// The chips draw their own rings, values and counts rather than handing
    /// them to an AppKit control, which renders a label as a button title and
    /// drops everything else. Assert they put something on screen.
    @Test("every chip draws")
    func everyChipDraws() async throws {
        let size = NSSize(width: 660, height: 44)
        let drawn = try await FindOnlineRendering.pixels(
            IdentifyingSignalChips(
                toolbar: PreviewData.toolbarIdentifying,
                onToggle: { _ in },
            ),
            size: size
        )
        let empty = try await FindOnlineRendering.pixels(
            IdentifyingSignalChips(
                toolbar: BridgeSignalsToolbar(signals: []),
                onToggle: { _ in },
            ),
            size: size
        )

        #expect(drawn != empty)
    }
}

@MainActor
@Suite("Adjusting a candidate's signals")
struct SignalAdjustPopoverTests {
    /// Clicking the disc ID or the barcode takes it in or out of the run. The
    /// catalog is not a toggle: it is chosen by value, so it sends its own.
    @Test("a signal row's click is the toggle for that signal")
    func aRowSendsItsOwnToggle() {
        func signal(_ kind: BridgeSignalKind) -> BridgeToolbarSignal {
            BridgeToolbarSignal(
                kind: kind,
                value: "value",
                origin: .artwork,
                state: .found(count: 1),
                excluded: false,
                options: []
            )
        }

        #expect(BridgeSignalToggle(signal: signal(.discId)) == .disc)
        #expect(BridgeSignalToggle(signal: signal(.barcode)) == .barcode)
        #expect(BridgeSignalToggle(signal: signal(.catalog)) == nil)
    }

    /// Every row draws itself — an AppKit checkbox renders its label as the
    /// button's title, which would leave the value and the count off the row
    /// entirely. Assert the rows put something on screen.
    @Test("the signal rows draw")
    func theSignalRowsDraw() async throws {
        let drawn = try await FindOnlineRendering.pixels(
            SignalAdjustPopover(
                toolbar: PreviewData.toolbarBothMatched,
                onToggle: { _ in },
                onRerun: {},
            )
        )
        let runAgainOnly = try await FindOnlineRendering.pixels(
            SignalAdjustPopover(
                toolbar: BridgeSignalsToolbar(signals: []),
                onToggle: { _ in },
                onRerun: {},
            )
        )

        #expect(drawn != runAgainOnly)
    }

    /// The catalog's numbers are rows of their own, each with its value and
    /// where it was read off.
    @Test("the catalog's numbers draw")
    func theCatalogNumbersDraw() async throws {
        let options = PreviewData.toolbarCatalogChoices.signals[1].options
        let drawn = try await FindOnlineRendering.pixels(
            CatalogOptionsList(options: options, onChoose: { _ in })
        )
        let none = try await FindOnlineRendering.pixels(
            CatalogOptionsList(options: [], onChoose: { _ in })
        )

        #expect(drawn != none)
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
