import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A Done row renders every library copy core associates with the folder.
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
            records: [],
            probablyInLibrary: false
        )
        let lines = try await renderedLines(row)
        #expect(lines.carrying("Album"))
        // Text recognition reads the separator as it pleases; the artist and
        // the year on the one line under the title are what is drawn.
        #expect(lines.carrying("Artist"))
        #expect(lines.carrying("1952"))
        #expect(!lines.carrying(PreviewData.folderCandidates[0].displayName))
    }

    @MainActor
    @Test("a folder only another release's hash matches draws the hint")
    func aFolderOnlyAHashMatchesDrawsTheHint() async throws {
        var row = PreviewData.importedRowIdentified
        row.probablyInLibrary = true
        var copy = row.releases[0]
        copy.releaseId = "second-copy"
        copy.title = "Second Album"
        row.releases.append(copy)

        let lines = try await renderedLines(row)
        #expect(lines.carrying(row.releases[0].title))
        #expect(lines.carrying(copy.title))
        #expect(lines.carrying(coreString("ui.import.probably_in_library")))
    }

    /// Every one of several imported folders is its own import, so none of
    /// their rows says the library probably has it.
    @MainActor
    @Test("a folder's own import draws no hint")
    func aFoldersOwnImportDrawsNoHint() async throws {
        let rows = [
            PreviewData.importedRowIdentified,
            PreviewData.importedRowFromTags,
            PreviewData.importedRow(
                for: PreviewData.folderCandidates[1],
                title: "Third Album",
                year: 2001,
                records: [],
                probablyInLibrary: false
            ),
        ]
        for row in rows {
            let lines = try await renderedLines(row)
            #expect(lines.carrying(row.releases[0].title))
            #expect(
                !lines.carrying(coreString("ui.import.probably_in_library")),
                "\(row.releases[0].title) is its own import"
            )
        }
    }

    @Test("a release with no year names its artist alone")
    func aReleaseWithNoYearNamesItsArtistAlone() {
        let summary = ImportReleaseSummary(
            release: PreviewData.importedRowFromTags.releases[0]
        )
        #expect(summary.title == "Album Title")
        #expect(summary.artist == "Artist Name")
    }

    @Test("a release with no title stands a placeholder in for it")
    func aReleaseWithNoTitleStandsAPlaceholderIn() {
        var release = PreviewData.importedRowFromTags.releases[0]
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
        undescribed.releases[0].records = []
        #expect(try await pixels(of: described) != pixels(of: undescribed))
        #expect(
            try await fittingSize(of: described)
                == fittingSize(of: undescribed),
            "a hidden arrow still holds its place"
        )
    }

    private static let rowSize = NSSize(width: 340, height: 180)

    /// `row` hosted for the length of `body`.
    @MainActor
    private func withHostedRow<Value>(
        _ row: BridgeImportedRow,
        _ body: (NSView) async throws -> Value
    ) async throws -> Value {
        try await SnapshotTestSupport.withHostedWindow(
            ImportedRowView(row: row, uploadObservations: [:], onReveal: {})
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
