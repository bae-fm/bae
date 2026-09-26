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

        let unidentified = try await fittingSize(
            of: PreviewData.triageRowUnidentified
        )
        let prefilled = try await fittingSize(
            of: PreviewData.triageRowPrefilledFromTags
        )
        #expect(unidentified.height >= TriageRowView.coverPointSize)
        #expect(unidentified.height == prefilled.height)
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
            try await fittingSize(of: read) == fittingSize(of: notRead),
            "a hidden arrow still holds its place"
        )
    }

    /// What a result asks is the pane's to state: a row draws the same
    /// whichever question its placement carries, and the same as a row that
    /// asks nothing.
    @MainActor
    @Test(
        "a row draws no badge for what its result asks",
        arguments: [
            BridgeNeedsYou.severalMatches(count: 2), .noMatch,
            .nothingToLookUp, .lookupFailed,
            .trackCountDisagrees(local: 13, source: 12), .sourceTracksUnknown,
        ]
    )
    func aRowDrawsNoBadgeForWhatItsResultAsks(_ reason: BridgeNeedsYou)
        async throws
    {
        var ready = PreviewData.triageRowReadFromRecord
        ready.placement = .ready
        var asking = ready
        asking.placement = .needsYou(reason: reason)
        #expect(try await pixels(of: asking) == pixels(of: ready))
    }

    /// Every question a row can ask resolves to a sentence from the app's
    /// `Core` table rather than falling back to its key.
    @Test(
        "every question resolves to its own sentence",
        arguments: [
            BridgeNeedsYou.severalMatches(count: 2), .noMatch,
            .nothingToLookUp, .lookupFailed,
            .trackCountDisagrees(local: 13, source: 12), .sourceTracksUnknown,
        ]
    )
    func everyQuestionResolvesToItsOwnSentence(_ reason: BridgeNeedsYou) {
        let text = reason.localizedText
        #expect(!text.isEmpty)
        #expect(text != bridgeNeedsYouKey(needsYou: reason))
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
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseRecordsCard(records: PreviewData.identifiedFromBothCatalogs)
                .preferredColorScheme(.light)
                .background(.white),
            size: Self.cardSize
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let png = try await SnapshotTestSupport.capturePNG(
                host,
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
    }

    private static let rowSize = NSSize(width: 340, height: 80)
    private static let cardSize = NSSize(
        width: ReleaseRecordsCard.width,
        height: 96
    )

    /// `row` hosted for the length of `body`.
    @MainActor
    private func withHostedRow<Value>(
        _ row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard,
        _ body: (NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            TriageRowView(
                row: row,
                coverContent: nil,
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
        ) { _, host in
            try await body(host)
        }
    }

    @MainActor
    private func fittingSize(of row: BridgeTriageRow) async throws -> NSSize {
        try await withHostedRow(row) { $0.fittingSize }
    }

    @MainActor
    private func pixels(
        of row: BridgeTriageRow,
        prominence: BackgroundProminence = .standard
    ) async throws -> Data {
        try await withHostedRow(row, prominence: prominence) { host in
            try await SnapshotTestSupport.capturePNG(host, size: Self.rowSize)
        }
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
