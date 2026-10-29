import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@MainActor
struct ReleaseStorageSheetTests {
    /// The sheet follows the release's live read, so a storage action's
    /// result shows in its status band while it stays open: here the
    /// release it opened on as Cloud is read again as Pinned.
    @Test("The Storage sheet shows the release as its read gives it now")
    func theSheetFollowsTheReleasesRead() async throws {
        let album = try #require(PreviewData.albumDetails["a-01"])
        var cloud = try #require(
            album.releases.first { $0.id == album.album.primaryReleaseId }
        )
        cloud.storageState = .remote
        cloud.pinned = false
        cloud.storageActions = [.pin]
        var pinned = cloud
        pinned.pinned = true
        pinned.storageActions = [.unpin]
        let (reads, deliver) = AsyncStream.makeStream(of: BridgeRelease.self)
        let delivered = Delivered(reads)
        let library = Library(
            releaseDetail: {
                DetailQuery(
                    setId: { _ in },
                    next: { try await delivered.next() },
                    cancel: {}
                )
            },
            releaseFiles: { .fixed([]) }
        )
        let size = NSSize(width: 640, height: 600)
        try await SnapshotTestSupport.withHostedWindow(
            ReleaseStorageSheet(
                release: ReleaseDetail(bridge: cloud),
                onAction: { _ in },
                onExport: {},
                onSaveAs: {},
                onDone: {}
            )
            .environment(library)
            .environment(OutboxStore(uploadsPaused: false))
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await waitUntilShown("Cloud", in: host, size: size)
            deliver.yield(pinned)
            try await waitUntilShown("Pinned", in: host, size: size)
        }
    }

    /// Wait for a line holding `word` to be drawn, saying what the sheet drew
    /// instead when it never is.
    private func waitUntilShown(_ word: String, in host: NSView, size: NSSize)
        async throws
    {
        var drawn: [String] = []
        do {
            try await Wait.until {
                let png = try await SnapshotTestSupport.capturePNG(
                    host,
                    size: size
                )
                drawn = try await SnapshotTestSupport.recognizedText(in: png)
                    .map(\.text)
                return drawn.contains { $0.contains(word) }
            }
        }
        catch {
            Issue.record("waiting for \(word), the sheet drew \(drawn)")
            throw error
        }
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
