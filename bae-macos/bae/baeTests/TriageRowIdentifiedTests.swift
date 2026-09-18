import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a candidate row shows about where its draft came from. Which reading a
/// row has is core's answer and core tests it; these are about what the row
/// draws once it has one.
@Suite("Triage row rendering")
struct TriageRowIdentifiedTests {
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
    @Test("a row filled from tags draws its draft and carries no mark")
    func prefilledRowCarriesNoMark() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowPrefilledFromTags
        )
        #expect(lines.carrying("Album Title Twelve"))
        #expect(lines.carrying("Artist Name"))
        #expect(!lines.carrying("Release Folder Twelve"))

        let summary = try #require(
            ImportReleaseSummary(row: PreviewData.triageRowPrefilledFromTags)
        )
        #expect(summary.records.isEmpty)
    }

    /// The row says *that* it is identified, once, on the title line. Which
    /// catalogs describe the release is the mark's hover — so the end of the
    /// row no longer names providers.
    @MainActor
    @Test("an identified row marks its title and badges no providers")
    func identifiedRowMarksItsTitle() async throws {
        let row = PreviewData.triageRowIdentifiedOnline
        let lines = try await renderedLines(row)
        #expect(lines.carrying("Album Title Thirteen"))
        #expect(!lines.carrying("MB"))
        #expect(!lines.carrying("Discogs"))

        let summary = try #require(ImportReleaseSummary(row: row))
        #expect(summary.records.map(\.catalog) == [.musicBrainz, .discogs])
        // The same row read as a plain draft draws the same words, so the mark
        // is the whole of the difference and the pixels are where it shows up.
        let marked = try await pixels(of: row)
        let unmarked = try await pixels(of: row.reading(.prefilled))
        #expect(marked != unmarked)
    }

    /// The mark and the placement's own tag are two different answers — one
    /// says the draft came from a source, the other what the row still needs —
    /// so a row with both shows both.
    @MainActor
    @Test("a row both identified and unsettled shows its mark and its tag")
    func identifiedRowKeepsItsPlacementTag() async throws {
        let row = PreviewData.triageRowIdentifiedSeveralMatches
        let lines = try await renderedLines(row)
        #expect(lines.carrying("2 matches"))

        let marked = try await pixels(of: row)
        let unmarked = try await pixels(of: row.reading(.prefilled))
        #expect(marked != unmarked)
    }

    /// Every catalog that describes the pressing the pick claimed.
    @MainActor
    @Test("the hover names every catalog that describes the release")
    func theHoverNamesEveryCatalog() async throws {
        let hosted = SnapshotTestSupport.hostInWindow(
            IdentifiedFromPopover(
                marks: [],
                verification: nil,
                records: PreviewData.identifiedFromBothCatalogs
            )
            .preferredColorScheme(.light)
            .background(.white),
            size: Self.popoverSize
        )
        await SnapshotTestSupport.settle(hosted.host)
        let png = try await SnapshotTestSupport.capturePNG(
            hosted.host,
            size: Self.popoverSize
        )
        let lines =
            try await SnapshotTestSupport.recognizedText(
                in: png,
                languages: ["en-US"]
            )
            .map(\.text)

        #expect(lines.carrying("MusicBrainz"))
        #expect(lines.carrying("Discogs"))
    }

    private static let rowSize = NSSize(width: 340, height: 80)
    private static let popoverSize = NSSize(width: 300, height: 96)

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
    private func pixels(of row: BridgeTriageRow) async throws -> Data {
        let host = hostedRow(row)
        await SnapshotTestSupport.settle(host)
        return try await SnapshotTestSupport.capturePNG(
            host,
            size: Self.rowSize
        )
    }

    @MainActor
    private func renderedLines(_ row: BridgeTriageRow) async throws -> [String]
    {
        try await SnapshotTestSupport.recognizedText(
            in: pixels(of: row),
            languages: ["en-US"]
        )
        .map(\.text)
    }
}

extension BridgeTriageRow {
    /// The same row read another way — what it would be had its draft come
    /// from somewhere else.
    fileprivate func reading(
        _ reading: BridgeTriageReading
    ) -> BridgeTriageRow {
        var copy = self
        copy.reading = reading
        return copy
    }
}
