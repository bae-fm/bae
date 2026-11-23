import BaeKit
import Testing

@testable import bae

/// A Done row is the row its folder was in Found: drawn by the same view,
/// with nothing added for being done. The row draws its summary and its
/// status line from the row; the rest of it is the
/// `import-done-rows-uploading` shot's.
@Suite("Done row rendering")
@MainActor
struct DoneRowTests {
    @Test(
        "a Done row draws exactly what its Found row drew",
        arguments: [
            PreviewData.triageRowDoneImported,
            PreviewData.triageRowDoneFromTags,
        ]
    )
    func aDoneRowDrawsItsFoundRow(done: BridgeTriageRow) {
        var found = done
        found.placement = .pending
        found.importStatus = nil
        #expect(
            ImportReleaseSummary(row: done)?.title
                == ImportReleaseSummary(row: found)?.title
        )
        #expect(
            ImportReleaseSummary(row: done)?.artist
                == ImportReleaseSummary(row: found)?.artist
        )
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

    @Test(
        "a row with an artist and a status reserves room for both",
        arguments: [
            BridgeTriageStateLine.queuedToUpload,
            .error(
                failure: BridgeInternalFailure(
                    detail: "Failed to store a lookup result"
                )
            ),
        ]
    )
    func rowsWithStatusReserveRoom(state: BridgeTriageStateLine) {
        var row = PreviewData.triageRowDoneImported
        let heightWithoutStatus = ImportCandidateTableLayout.height(for: row)
        row.stateLine = state
        #expect(
            ImportCandidateTableLayout.height(for: row) > heightWithoutStatus
        )
    }
}
