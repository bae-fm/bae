import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// What a Done row draws: the library release the candidate became, as core
/// read it from the library. Which release that is, and that an edit to it
/// reaches the row, is core's and core tests it; these are about what the row
/// draws from it.
@Suite("Done row rendering")
struct ImportedRowViewTests {
    @MainActor
    @Test("a Done row draws its release's title, artist and year")
    func aDoneRowDrawsItsRelease() async throws {
        let row = PreviewData.importedRow(
            for: PreviewData.folderCandidates[0],
            title: "Album",
            artist: "Artist",
            year: 1952,
            records: []
        )
        let lines = try await renderedLines(row)
        #expect(lines.carrying("Album"))
        // Text recognition reads the separator as it pleases; the artist and
        // the year on the one line under the title are what is drawn.
        #expect(lines.carrying("Artist"))
        #expect(lines.carrying("1952"))
        #expect(!lines.carrying(PreviewData.folderCandidates[0].displayName))
    }

    @Test("a release with no year names its artist alone")
    func aReleaseWithNoYearNamesItsArtistAlone() {
        let summary = ImportReleaseSummary(
            release: PreviewData.importedRowFromTags.release
        )
        #expect(summary.title == "Album Title")
        #expect(summary.artist == "Artist Name")
    }

    @Test("a release with no title stands a placeholder in for it")
    func aReleaseWithNoTitleStandsAPlaceholderIn() {
        var release = PreviewData.importedRowFromTags.release
        release.title = ""
        release.artist = nil
        let summary = ImportReleaseSummary(release: release)
        #expect(summary.titleIsPlaceholder)
        #expect(summary.artist == nil)
    }

    /// The arrow is what a release a catalog describes draws, and hiding it
    /// leaves the row's size alone.
    @MainActor
    @Test("the arrow marks a release a catalog describes")
    func theArrowMarksAReleaseACatalogDescribes() async throws {
        let described = PreviewData.importedRowIdentified
        var undescribed = described
        undescribed.release.records = []
        #expect(try await pixels(of: described) != pixels(of: undescribed))
        #expect(
            try await fittingSize(of: described)
                == fittingSize(of: undescribed),
            "a hidden arrow still holds its place"
        )
    }

    private static let rowSize = NSSize(width: 340, height: 80)

    /// `row` hosted for the length of `body`.
    @MainActor
    private func withHostedRow<Value>(
        _ row: BridgeImportedRow,
        _ body: (NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            ImportedRowView(row: row, uploadObservation: nil, onReveal: {})
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
    private func fittingSize(of row: BridgeImportedRow) async throws -> NSSize {
        try await withHostedRow(row) { $0.fittingSize }
    }

    @MainActor
    private func pixels(of row: BridgeImportedRow) async throws -> Data {
        try await withHostedRow(row) { host in
            try await SnapshotTestSupport.capturePNG(host, size: Self.rowSize)
        }
    }

    @MainActor
    private func renderedLines(_ row: BridgeImportedRow) async throws
        -> [String]
    {
        try await SnapshotTestSupport.recognizedText(
            in: pixels(of: row),
            languages: ["en-US"]
        )
        .map(\.text)
    }
}
