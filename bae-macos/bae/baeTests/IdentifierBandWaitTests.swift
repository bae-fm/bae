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
    @Test("every wait in the band is named")
    func waitsAreNamed() async throws {
        let run = BridgeIdentifyRun(
            providers: [.musicBrainz],
            discId: .reading,
            barcode: .rows(
                scanning: true,
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
            catalog: .numbers(scanning: true, rows: [], candidates: []),
            isrc: .absent,
            search: .waiting(album: "Album Title", artist: "Artist Name")
        )
        let size = NSSize(width: 1_400, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            IdentifierBand(
                run: run,
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
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            // The pixels the words were read from, kept with the test's
            // results: a runner can read them differently from this Mac.
            Attachment.record(png, named: "Identifier band.png")
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            for wait in ["Searching", "Waiting", "Reading artwork"] {
                #expect(lines.carrying(wait), "drew \(lines) without \(wait)")
            }
            // The Disc ID chip's read, beside the artwork's.
            let reads =
                lines.joined(separator: " ")
                .components(separatedBy: "Reading").count - 1
            #expect(reads == 2, "drew \(lines)")
        }
    }

    @Test("the search form says the artwork is still being read")
    func formNamesTheArtworkRead() async throws {
        let size = NSSize(width: 900, height: 90)
        try await SnapshotTestSupport.withHostedWindow(
            ImportSearchFormView(
                form: CandidateSearchState(),
                onCommit: { _ in },
                signals: Signals(
                    text: .scanning(catalogs: [], freeText: ["Album Title"])
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
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            Attachment.record(png, named: "Search form.png")
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(
                lines.carrying("Reading the artwork for suggestions"),
                "drew \(lines)"
            )
        }
    }
}
