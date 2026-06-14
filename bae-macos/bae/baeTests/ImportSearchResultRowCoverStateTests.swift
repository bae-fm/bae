import AppKit
import SwiftUI
import Testing

@testable import bae

@Suite("ImportSearchResultRow cover states")
struct ImportSearchResultRowCoverStateTests {
    @MainActor
    @Test(
        "missing, loading, and failed import result covers render distinct pixels"
    )
    func importResultCoverStatesRenderDistinctPixels() async throws {
        let unavailable = try await renderImportSearchResultRow(
            coverUrl: nil,
            mediaPaths: MediaPaths.stub
        )
        let loading = try await renderImportSearchResultRow(
            coverUrl: "loading",
            mediaPaths: MediaPaths(
                fetchCoverBytes: { _ in
                    try await Task.sleep(nanoseconds: 2_000_000_000)
                    return Data()
                }
            ),
            waitNanoseconds: 50_000_000
        )
        let failed = try await renderImportSearchResultRow(
            coverUrl: "failed",
            mediaPaths: MediaPaths(
                fetchCoverBytes: { _ in throw StubError.notImplemented }
            ),
            waitNanoseconds: 50_000_000
        )

        #expect(unavailable != loading)
        #expect(unavailable != failed)
        #expect(loading != failed)
    }

    @MainActor
    private func renderImportSearchResultRow(
        coverUrl: String?,
        mediaPaths: MediaPaths,
        waitNanoseconds: UInt64 = 0
    ) async throws
        -> Data
    {
        try await SnapshotTestSupport.renderPNG(
            ImportSearchResultRow(
                result: MetadataResult(
                    bridge: BridgeMetadataResult(
                        source: .musicBrainz,
                        releaseId: "release-id",
                        title: "Album Title",
                        artist: "Artist Name",
                        year: nil,
                        format: nil,
                        label: nil,
                        catalogNumber: nil,
                        country: nil,
                        coverUrl: coverUrl,
                        sourceGroupId: nil,
                        releaseUrl: nil,
                    )
                ),
                isImporting: false,
                libraryStatus: nil,
                onCommit: { _ in },
            )
            .frame(width: 360, height: 80, alignment: .topLeading)
            .environment(UiStore())
            .environment(mediaPaths)
            .environment(\.colorScheme, .dark),
            size: NSSize(width: 360, height: 80),
            waitNanoseconds: waitNanoseconds
        )
    }
}
