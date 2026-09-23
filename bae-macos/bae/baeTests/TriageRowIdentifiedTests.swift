import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a candidate row draws about its release. Which reading a row holds is
/// core's answer and core tests it; these are about the arrow the row draws
/// from it, and about the card the library's facts line opens.
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
    @Test("a row filled from tags draws its draft and no arrow")
    func prefilledRowDrawsItsDraft() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowPrefilledFromTags
        )
        #expect(lines.carrying("Album Title Twelve"))
        #expect(lines.carrying("Artist Name"))
        #expect(!lines.carrying("Release Folder Twelve"))
    }

    /// The word is never drawn: what the arrow says is said by the arrow.
    @MainActor
    @Test("a row states where its facts came from without naming it")
    func aRowNamesNeitherFact() async throws {
        let lines = try await renderedLines(
            PreviewData.triageRowReadFromRecord
        )
        #expect(lines.carrying("Album Title Fifteen"))
        #expect(!lines.carrying(coreString("core.identity.identified")))
    }

    /// The arrow is what a row read from a record draws and a row read from
    /// anywhere else does not, and hiding it leaves the row's height alone.
    @MainActor
    @Test("the arrow marks a row whose facts came from a record")
    func theArrowMarksARowReadFromARecord() async throws {
        let read = PreviewData.triageRowReadFromRecord
        let notRead = PreviewData.triageRowNotReadFromRecord
        #expect(try await pixels(of: read) != pixels(of: notRead))
        #expect(
            hostedRow(read).fittingSize == hostedRow(notRead).fittingSize,
            "a hidden arrow still holds its place"
        )
    }

    /// A row flagging pressings a run found has settled on no record, so it
    /// draws no arrow and draws the unread question's chip.
    @MainActor
    @Test("a row flagging several pressings draws no arrow")
    func aRowFlaggingSeveralPressingsDrawsNoArrow() async throws {
        let row = PreviewData.triageRowSeveralMatches
        #expect(try await renderedLines(row).carrying("3 matches"))
        #expect(
            try await pixels(of: row)
                != pixels(of: PreviewData.triageRowReadFromRecord)
        )
    }

    /// The chip is the unread marker and nothing else: a row whose question
    /// has been read draws none, whatever its placement asks.
    @MainActor
    @Test("the chip is drawn from the unread question alone")
    func theChipIsDrawnFromTheUnreadQuestionAlone() async throws {
        var read = PreviewData.triageRowPickAPressing
        read.attention = nil
        let readLines = try await renderedLines(read)
        #expect(!readLines.carrying("2 matches"))

        var unread = PreviewData.triageRowNotReadFromRecord
        unread.attention = .severalMatches(count: 2)
        #expect(try await renderedLines(unread).carrying("2 matches"))
    }

    /// On a selected row the whole text column goes white, and the arrow
    /// follows it rather than keeping its own colour.
    @MainActor
    @Test("a selected row draws its arrow in the column's colour")
    func aSelectedRowDrawsItsArrowInTheColumnsColour() async throws {
        let row = PreviewData.triageRowReadFromRecord
        let resting = try await pixels(of: row)
        let selected = try await pixels(of: row, prominence: .increased)
        #expect(resting != selected)
    }

    /// The card behind the library's facts line names every catalog that
    /// describes the release — its only way to those links.
    @MainActor
    @Test("the card names every catalog that describes the release")
    func theCardNamesEveryCatalog() async throws {
        let hosted = SnapshotTestSupport.hostInWindow(
            ReleaseRecordsCard(records: PreviewData.identifiedFromBothCatalogs)
                .preferredColorScheme(.light)
                .background(.white),
            size: Self.cardSize
        )
        await SnapshotTestSupport.settle(hosted.host)
        let png = try await SnapshotTestSupport.capturePNG(
            hosted.host,
            size: Self.cardSize
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
    private static let cardSize = NSSize(
        width: ReleaseRecordsCard.width,
        height: 96
    )

    @MainActor
    private func hostedRow(
        _ row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard
    ) -> NSView {
        SnapshotTestSupport.hostInWindow(
            TriageRowView(
                row: row,
                coverContent: nil,
                uploadObservation: nil,
                isGroupMember: false,
                onReveal: {},
                onSkip: { _ in }
            )
            .environment(\.backgroundProminence, prominence)
            .environment(ImageStore.stub())
            .preferredColorScheme(.light)
            .background(.white)
            .frame(width: Self.rowSize.width),
            size: Self.rowSize
        )
        .host
    }

    @MainActor
    private func pixels(
        of row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard
    ) async throws -> Data {
        let host = hostedRow(row, prominence: prominence)
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
