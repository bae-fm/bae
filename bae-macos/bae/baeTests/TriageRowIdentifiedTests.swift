import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a candidate row draws about its release. Which reading a row holds is
/// core's answer and core tests it; these are about the summary and the arrow
/// the row draws from it. Which line an unidentified row draws, and the card
/// the library's facts line opens, are the `import-release-queue` and
/// `release-records-card` shots', and how the arrow looks the
/// `import-rows-record-arrow` shot's.
@Suite("Triage row rendering")
struct TriageRowIdentifiedTests {
    @MainActor
    @Test("an unidentified row keeps its cover tile")
    func unidentifiedRowKeepsItsCoverTile() async throws {
        let unidentified = try await fittingSize(
            of: PreviewData.triageRowUnidentified
        )
        let prefilled = try await fittingSize(
            of: PreviewData.triageRowPrefilledFromTags
        )
        #expect(unidentified.height >= MacImportRow.coverPointSize)
        #expect(unidentified.height == prefilled.height)
    }

    @MainActor
    @Test("a row filled from tags summarizes its draft")
    func prefilledRowSummarizesItsDraft() throws {
        let summary = try #require(
            ImportReleaseSummary(row: PreviewData.triageRowPrefilledFromTags)
        )
        #expect(summary.title == "Album Title Twelve")
        #expect(summary.artist == "Artist Name")
    }

    /// The summary names the album and its artist, and nothing about where
    /// they came from: what the arrow says is said by the arrow.
    @MainActor
    @Test("a row read from a record summarizes its draft alone")
    func aRowReadFromARecordSummarizesItsDraft() throws {
        let summary = try #require(
            ImportReleaseSummary(row: PreviewData.triageRowReadFromRecord)
        )
        #expect(summary.title == "Album Title Fifteen")
        #expect(summary.artist == "Artist Name")
    }

    /// The arrow shows on a row read from a record and is hidden on a row
    /// read from anywhere else, and hiding it leaves the row's height alone.
    @MainActor
    @Test("the arrow marks a row whose facts came from a record")
    func theArrowMarksARowReadFromARecord() async throws {
        let read = PreviewData.triageRowReadFromRecord
        let notRead = PreviewData.triageRowNotReadFromRecord
        #expect(read.reading.readFromRecord)
        #expect(!notRead.reading.readFromRecord)
        #expect(
            try await fittingSize(of: read) == fittingSize(of: notRead),
            "a hidden arrow still holds its place"
        )
    }

    /// Every folder check resolves to a sentence from the app's `Core` table
    /// rather than falling back to its key.
    @Test(
        "every folder check resolves to its own sentence",
        arguments: [
            BridgeFolderCheck.sourceTracksUnknown,
            .mediumDisagrees(folder: .cdRip),
            .mediumDisagrees(folder: .notCdAudio(sampleRateHz: 96_000)),
        ]
    )
    func everyFolderCheckResolvesToItsOwnSentence(
        _ folderCheck: BridgeFolderCheck
    ) throws {
        let text = try #require(folderCheck.localizedText)
        #expect(!text.isEmpty)
        #expect(text != bridgeFolderCheckKey(folderCheck: folderCheck))
        #expect(!text.contains("%"), "every operand is filled in")
    }

    /// Audio no CD holds is stated with its rate, which a row does not have
    /// at hand: without it there is no sentence to show.
    @Test("audio no CD holds needs its rate to be stated")
    func audioNoCdHoldsNeedsItsRateToBeStated() {
        let stated = BridgeFolderCheck.mediumDisagrees(
            folder: .notCdAudio(sampleRateHz: 96_000)
        )
        #expect(stated.localizedText?.contains("96") == true)
        #expect(
            BridgeFolderCheck.mediumDisagrees(
                folder: .notCdAudio(sampleRateHz: nil)
            )
            .localizedText == nil
        )
    }

    private static let rowSize = NSSize(width: 340, height: 80)

    /// `row` hosted for the length of `body`.
    @MainActor
    private func withHostedRow<Value>(
        _ row: BridgeTriageRow,
        _ body: (NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            MacImportRow(
                row: row,
                coverContent: nil,
                isGroupMember: false
            )
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
}
