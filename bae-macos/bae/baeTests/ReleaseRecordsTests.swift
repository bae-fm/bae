import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// Which catalogs describe a release is one answer, drawn once: as a row of
/// links last in the import pane, and behind the library expansion's facts
/// line. The row's names are the `release-records-every-catalog` shot's, and
/// the pane drawing them once, in that row, the `import-metadata-draft`
/// shot's.
@MainActor
@Suite("Release records")
struct ReleaseRecordsTests {
    private static let cardSize = NSSize(width: 1100, height: 760)

    /// The facts line carries the record arrow at rest when a catalog
    /// describes the release, and nothing when none does — the one thing on
    /// the line that says where its facts were read.
    @Test("the expansion line draws its arrow only when records describe it")
    func theExpansionDrawsItsArrowOnlyWhenDescribed() async throws {
        let detail = PreviewData.albumDetail(albumId: "a-01")
        let described = detail.primaryRelease
        #expect(!described.records.isEmpty, "the fixture carries records")
        var undescribed = described
        undescribed.records = []

        let withRecords = try await pixels(of: described, in: detail)
        let without = try await pixels(of: undescribed, in: detail)
        // Compared as one answer rather than as two collections: an
        // inequality between megabyte-sized captures is not a diff worth
        // computing.
        let identical = withRecords == without
        #expect(!identical, "the records put the arrow on the line")
    }

    private func pixels(
        of release: ReleaseDetail,
        in detail: AlbumDetail
    ) async throws -> Data {
        return try await SnapshotTestSupport.withHostedWindow(
            PreviewData.albumExpansionContent(
                detail: detail,
                selectedRelease: release,
                releaseCursor: .constant(
                    PreviewData.releaseCursor(
                        detail: detail,
                        preferring: release.id
                    )
                )
            )
            .frame(width: Self.cardSize.width)
            .preferredColorScheme(.light)
            .background(.white)
            .environment(UiStore())
            .environment(ImageStore.stub()),
            size: Self.cardSize
        ) { _, host in
            return try await SnapshotTestSupport.capturePNG(
                host,
                size: Self.cardSize
            )
        }
    }
}
