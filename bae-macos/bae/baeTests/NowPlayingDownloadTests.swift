import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

/// A track whose load waits on storage can take a while, so the now-playing
/// bar names that wait over how much of it is here, in place of the seek bar,
/// and nothing in the bar spins.
@MainActor
@Suite("Now-playing download")
struct NowPlayingDownloadTests {
    @Test("a load waiting on storage is named, without a spinner")
    func downloadIsNamed() async throws {
        let downloads = LatestStream.trackDownloads(holding: [
            "track-1": BridgeTrackDownload(here: 1_000_000, total: 4_000_000)
        ])
        let size = NSSize(width: 1_100, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            bar(download: downloads.source(for: "track-1"))
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(
                lines.carrying("Downloading the track"),
                "drew \(lines)"
            )
        }
    }

    @Test("a load with nothing to download draws no spinner and no line")
    func localLoadShowsNothing() async throws {
        let size = NSSize(width: 1_100, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            bar(download: LatestStream.trackDownloads().source(for: "track-1"))
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let spinners = SnapshotTestSupport.descendants(of: host)
                .filter { $0 is NSProgressIndicator }
            #expect(spinners.isEmpty, "drew \(spinners)")
            let png = try await SnapshotTestSupport.capturePNG(host, size: size)
            let lines = try await SnapshotTestSupport.recognizedText(in: png)
                .map(\.text)
            #expect(!lines.carrying("Downloading"), "drew \(lines)")
        }
    }

    /// The bar for a track loading to play, with `download` the stream of
    /// what its load waits on.
    private func bar(download: LatestSource<BridgeTrackDownload>) -> some View {
        NowPlayingBar(
            trackTitle: "Track Title",
            secondaryLine: "Artist Name",
            cover: nil,
            isPlaying: true,
            position: nil,
            download: download,
            showRemainingTime: false,
            volume: 0.7,
            isMuted: false,
            repeatMode: .off,
            shuffled: nil,
            showQueue: false,
            onPlayPause: {},
            onNext: {},
            onPrevious: {},
            onSeek: { _ in },
            onToggleRemainingTime: {},
            onVolumeChange: { _ in },
            onToggleMute: {},
            onSetShuffle: { _ in },
            onSetQueuePresented: { _ in },
            onCycleRepeat: {},
            onDropToQueue: { _ in },
            onNavigateToAlbum: {},
            queueAddPublisher: Empty().eraseToAnyPublisher(),
            castControl: AnyView(EmptyView())
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 0, context: nil))
        .environment(Queue.stub())
    }
}
