import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A run's reads and lookups can take a while, so each chip says what it
/// waits on, in words, where a spinner stood.
@MainActor
@Suite("Identifier band waits")
struct IdentifierBandWaitTests {
    /// A run waiting on everything the band names: a Disc ID and ISRCs still
    /// being read, a barcode being asked about, the title search waiting, and
    /// the artwork still being read.
    private static let waitingRun = BridgeIdentifyRun(
        providers: [.musicBrainz],
        discId: .reading,
        barcode: .rows(
            rows: [
                BridgeSignalValueRow(
                    value: "0123456789012",
                    excluded: false,
                    cells: [
                        BridgeProviderCell(
                            source: .musicBrainz,
                            lookup: .lookingUp
                        )
                    ]
                )
            ]
        ),
        catalog: .numbers(rows: [], candidates: []),
        isrc: .reading,
        search: .waiting(album: "Album Title", artist: "Artist Name"),
        readingArtwork: true
    )

    @Test("every wait in the band is named")
    func waitsAreNamed() {
        let run = Self.waitingRun
        #expect(ChipWait(waitingOn: run.discId)?.words == "Reading…")
        #expect(ChipWait(waitingOn: run.isrc)?.words == "Reading…")
        #expect(ChipWait(waitingOn: run.search)?.words == "Waiting…")
        #expect(
            ChipWait(waitingOn: BridgeLookupState.lookingUp)?.words
                == "Searching…"
        )
        #expect(
            ChipWait(readingArtworkOf: run)?.words == "Reading artwork…"
        )
    }

    @Test("a chip past its wait names none")
    func settledStatesWaitOnNothing() {
        #expect(ChipWait(waitingOn: BridgeDiscIdStep.absent) == nil)
        #expect(
            ChipWait(
                waitingOn: BridgeIsrcStep.read(
                    isrcs: ["XX0000000001"],
                    lookup: .noMatch
                )
            ) == nil
        )
        #expect(ChipWait(waitingOn: BridgeSearchStep.notNeeded) == nil)
        #expect(ChipWait(waitingOn: BridgeLookupState.queued) == nil)
        #expect(ChipWait(readingArtworkOf: PreviewData.identifyRunFound) == nil)
    }

    @Test("the band waits without a spinner")
    func bandDrawsNoSpinner() async throws {
        let size = NSSize(width: 1_400, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            MacIdentifierBand(
                run: Self.waitingRun,
                onToggleLookup: { _ in },
                onToggleCatalogAgreement: { _ in },
                onRetryFailed: {},
                onEditTitleSearch: { _, _ in }
            )
            .frame(width: size.width, height: size.height, alignment: .top)
            .importPreviewEnvironment(),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
        }
    }

    @Test("the search form says the artwork is still being read")
    func formNamesTheArtworkRead() {
        let scanning = BridgeTextSignal.scanning(
            catalogs: [],
            freeText: ["Album Title"]
        )
        for tab in [SearchTab.general, .catalogNumber] {
            #expect(
                ImportSearchFormView.artworkReadNote(scanning, tab: tab)
                    == "Reading the artwork for suggestions…"
            )
        }
        // The barcode field takes no suggestions.
        #expect(
            ImportSearchFormView.artworkReadNote(scanning, tab: .barcode)
                == nil
        )
        #expect(
            ImportSearchFormView.artworkReadNote(
                .settled(catalogs: [], freeText: ["Album Title"]),
                tab: .general
            ) == nil
        )
    }

    @Test("the search form waits without a spinner")
    func formDrawsNoSpinner() async throws {
        let size = NSSize(width: 900, height: 90)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSearchFormView(
                form: CandidateSearchState(),
                onCommit: { _ in },
                signals: Signals(
                    bridge: BridgeSignals(
                        text: .scanning(catalogs: [], freeText: ["Album Title"])
                    )
                ),
                focusRequest: 0,
                onSearch: { _ in }
            )
            .frame(width: size.width, height: size.height, alignment: .top),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
        }
    }
}
