import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// A Done row is the row its folder was in Found: drawn by the same view,
/// with nothing added for being done.
@Suite("Done row rendering", .serialized)
@MainActor
struct DoneRowTests {
    private static let rowSize = NSSize(width: 340, height: 80)

    @Test(
        "a Done row draws exactly what its Found row drew",
        arguments: [
            PreviewData.triageRowDoneImported,
            PreviewData.triageRowDoneFromTags,
        ]
    )
    func aDoneRowDrawsItsFoundRow(done: BridgeTriageRow) async throws {
        var found = done
        found.placement = .pending
        found.importStatus = nil
        #expect(try await pixels(of: done) == pixels(of: found))
    }

    @Test("a Done row names the release imported from it")
    func aDoneRowNamesItsRelease() {
        #expect(
            PreviewData.triageRowDoneImported.importedRelease
                == BridgeImportedRelease(
                    releaseId: "preview-release",
                    albumId: "preview-album"
                )
        )
        #expect(PreviewData.triageRowPrefilledFromTags.importedRelease == nil)
    }

    private func pixels(of row: BridgeTriageRow) async throws -> Data {
        try await SnapshotTestSupport.withHostedWindow(
            TriageRowView(row: row, coverContent: nil, isGroupMember: false)
                .candidateReaderPreviewEnvironment()
                .environment(ImageStore.stub())
                .preferredColorScheme(.light)
                .background(.white)
                .frame(width: Self.rowSize.width),
            size: Self.rowSize
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            return try await SnapshotTestSupport.capturePNG(
                host,
                size: Self.rowSize
            )
        }
    }
}
