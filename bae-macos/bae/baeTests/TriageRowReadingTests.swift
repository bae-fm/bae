import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Triage row readings")
struct TriageRowReadingTests {
    @Test("a row with no draft is its folder and nothing else")
    func rowWithoutADraftIsUnidentified() {
        #expect(
            TriageRowReading.of(PreviewData.triageRowUnidentified)
                == .unidentified
        )
    }

    @Test("a draft read off the file tags names no source")
    func draftFromFileTagsIsPrefilled() {
        #expect(
            TriageRowReading.of(PreviewData.triageRowPrefilledFromTags)
                == .prefilled
        )
    }

    /// A pick pairs one source's release with another's into one pressing, and
    /// the row claims both — in the order every surface lists sources in,
    /// whichever of them the draft was read from.
    @Test("a pick names every source it claims, in the fixed order")
    func pickNamesEverySourceItClaims() {
        let ordered = bridgeMetadataSources()
        #expect(
            TriageRowReading.of(PreviewData.triageRowIdentifiedOnline)
                == .identified(ordered)
        )

        let ledByPartner = PreviewData.triageRowIdentifiedOnline.withProvenance(
            .externalRelease(
                source: .discogs,
                releaseId: "discogs-paired",
                partners: [
                    BridgeMetadataRef(
                        source: .musicBrainz,
                        releaseId: "rel-paired"
                    )
                ]
            )
        )
        #expect(TriageRowReading.of(ledByPartner) == .identified(ordered))
    }
}

@Suite("Triage row rendering")
struct TriageRowRenderingTests {
    @MainActor
    @Test("an unidentified row draws its folder, with its cover tile kept")
    func unidentifiedRowDrawsItsFolder() async throws {
        let lines = try await renderedLines(PreviewData.triageRowUnidentified)
        #expect(lines.carrying("Release Folder Eleven"))
        #expect(!lines.carrying("Album Title"))

        let unidentified = hostedRow(PreviewData.triageRowUnidentified)
        let prefilled = hostedRow(PreviewData.triageRowPrefilledFromTags)
        #expect(
            unidentified.fittingSize.height >= TriageRowView.coverPointSize
        )
        #expect(
            unidentified.fittingSize.height == prefilled.fittingSize.height
        )
    }

    @MainActor
    @Test("a row filled from tags draws its draft and names no source")
    func prefilledRowNamesNoSource() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowPrefilledFromTags
        )
        #expect(lines.carrying("Album Title Twelve"))
        #expect(lines.carrying("Artist Name"))
        #expect(!lines.carrying("Release Folder Twelve"))
        #expect(!lines.carrying("MB"))
        #expect(!lines.carrying("Discogs"))
    }

    /// The badges are the row's trailing column, not part of the release: the
    /// artist line says the artist and stops.
    @MainActor
    @Test("an identified row badges its sources away from the artist")
    func identifiedRowBadgesItsSources() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowIdentifiedOnline
        )
        #expect(lines.carrying("Album Title Thirteen"))
        #expect(lines.carrying("MB"))
        #expect(lines.carrying("Discogs"))

        let artistLines = lines.filter { $0.contains("Artist Name") }
        #expect(!artistLines.isEmpty)
        #expect(!artistLines.carrying("MB"))
        #expect(!artistLines.carrying("Discogs"))
    }

    /// The badges and the placement's own tag are two different answers — one
    /// says where the draft came from, the other what the row still needs —
    /// so a row with both shows both.
    @MainActor
    @Test("a row both identified and unsettled shows its badges and its tag")
    func identifiedRowKeepsItsPlacementTag() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowIdentifiedSeveralMatches
        )
        #expect(lines.carrying("MB"))
        #expect(lines.carrying("Discogs"))
        #expect(lines.carrying("2 matches"))
    }

    private static let rowSize = NSSize(width: 340, height: 80)

    @MainActor
    private func hostedRow(_ row: BridgeTriageRow) -> NSView {
        SnapshotTestSupport.hostInWindow(
            TriageRowView(
                row: row,
                coverContent: nil,
                uploadObservation: nil,
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(ImageStore.stub())
            .preferredColorScheme(.light)
            .background(.white)
            .frame(width: Self.rowSize.width),
            size: Self.rowSize
        )
        .host
    }

    @MainActor
    private func renderedLines(_ row: BridgeTriageRow) async throws -> [String]
    {
        let host = hostedRow(row)
        await SnapshotTestSupport.settle(host)
        let png = try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.rowSize
        )
        return
            try await SnapshotTestSupport.recognizedText(
                in: png,
                languages: ["en-US"]
            )
            .map(\.text)
    }
}

extension BridgeTriageRow {
    /// The same row with another provenance — what a pick led by the other
    /// source would have left on it.
    fileprivate func withProvenance(
        _ provenance: BridgeMetadataProvenance
    ) -> BridgeTriageRow {
        var copy = self
        copy.metadataProvenance = provenance
        return copy
    }
}
