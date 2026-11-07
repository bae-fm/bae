import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ReleaseStorageSheetTests {
    /// The sheet follows the release's live read, so a storage action's
    /// result shows in its status band while it stays open: here the
    /// release it opened on at rest on this Mac is read again uploading, and
    /// the band draws the upload's native bar. The band's words for each
    /// state are the `release-storage-sheet` shot's.
    @Test("The Storage sheet shows the release as its read gives it now")
    func theSheetFollowsTheReleasesRead() async throws {
        let album = try #require(PreviewData.albumDetails["a-01"])
        var resting = try #require(
            album.releases.first { $0.id == album.album.primaryReleaseId }
        )
        resting.storageState = .onOneDevice
        resting.storageActions = [.upload]
        resting.upload = nil
        resting.transitioning = false
        var uploading = resting
        uploading.storageActions = []
        uploading.transitioning = true
        uploading.upload = BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: 0,
                uploading: 1,
                activity: .uploading
            ),
            summary: [],
            wait: nil
        )
        let (reads, deliver) = AsyncStream.makeStream(of: BridgeRelease.self)
        let delivered = Delivered(reads)
        let library = Library(
            releaseDetail: {
                DetailQuery(
                    setId: { _ in },
                    values: PulledValues(read: { try await delivered.next() })
                )
            },
            releaseFiles: { .fixed([], answers: FixedAnswers()) }
        )
        let size = NSSize(width: 640, height: 600)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseStorageSheet(
                release: ReleaseDetail(bridge: resting),
                onAction: { _ in },
                onExport: {},
                onSaveAs: {},
                onDone: {}
            )
            .environment(library)
            .environment(OutboxStore(uploadsPaused: false))
            .environment(DownloadStore())
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            #expect(uploadBars(in: host) == 0)
            deliver.yield(uploading)
            try await Wait.untilLaidOut(host) { uploadBars(in: host) == 1 }
        }
    }

    /// The native bars the band draws for a transfer.
    private func uploadBars(in host: NSView) -> Int {
        SnapshotTestSupport.descendants(of: host)
            .filter { $0 is StreamBarNSView }
            .count
    }

    /// The releases a test hands its read, one per delivery.
    private final class Delivered: @unchecked Sendable {
        private var iterator: AsyncStream<BridgeRelease>.AsyncIterator

        init(_ stream: AsyncStream<BridgeRelease>) {
            iterator = stream.makeAsyncIterator()
        }

        func next() async throws -> DetailDelivery<BridgeRelease> {
            guard let release = await iterator.next() else {
                throw CancellationError()
            }
            return DetailDelivery(id: release.id, value: release)
        }
    }
}
