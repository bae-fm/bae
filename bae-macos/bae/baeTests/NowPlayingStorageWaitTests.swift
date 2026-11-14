import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

/// The seek bar stays in the now-playing bar while a track loads, drawing
/// the parts of the track whose bytes are here, and an arc turns around Play
/// while the load waits on storage.
@MainActor
@Suite("Now-playing storage wait")
struct NowPlayingStorageWaitTests {
    @Test("a load waiting on storage turns the arc and keeps the seek bar")
    func waitTurnsTheArc() async throws {
        let waits = LatestStream<Bool>.trackStorageWaits(waiting: ["track-1"])
        let held = [BridgeTrackSpan(start: 0, end: 0.08)]
        let buffered = LatestStream.trackBuffered(
            holding: ["track-1": BridgeTrackBuffered(ranges: held)]
        )
        let size = NSSize(width: 1_100, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            bar(
                buffered: buffered.source(for: "track-1"),
                storageWait: waits.source(for: "track-1")
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let descendants = SnapshotTestSupport.descendants(of: host)
            let arc = try #require(
                descendants.compactMap { $0 as? StorageWaitArcNSView }.first
            )
            #expect(!arc.isHidden)
            #expect(arc.accessibilityLabel() == "Downloading the track…")
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            #expect(slider.held == held)
        }
    }

    @Test("a load waiting on nothing shows no arc")
    func noWaitNoArc() async throws {
        let size = NSSize(width: 1_100, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            bar(
                buffered: LatestStream.trackBuffered().source(for: "track-1"),
                storageWait: LatestStream<Bool>.trackStorageWaits()
                    .source(for: "track-1")
            )
            .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let arcs = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? StorageWaitArcNSView }
            #expect(arcs.allSatisfy { $0.isHidden }, "drew \(arcs)")
        }
    }

    /// The bar for a track loading to play a quarter of the way in.
    private func bar(
        buffered: LatestSource<BridgeTrackBuffered>,
        storageWait: LatestSource<Bool>
    ) -> some View {
        MacNowPlayingBar(
            trackTitle: "Track Title",
            secondaryLine: "Artist Name",
            cover: nil,
            isPlaying: true,
            position:
                LatestStream.trackPositions(
                    holding: [
                        "track-1": BridgeTrackPosition(
                            positionMs: 0,
                            rate: 0,
                            anchoredAtNs: 0,
                            durationMs: 200_000
                        )
                    ]
                )
                .source(for: "track-1"),
            buffered: buffered,
            storageWait: storageWait,
            showRemainingTime: false,
            settings: PlaybackSettings(
                volume: 0.7,
                isMuted: false,
                repeatMode: .off
            ),
            receiverWithoutVolume: nil,
            shuffled: false,
            showQueue: false,
            onPlayPause: {},
            onNext: {},
            onPrevious: {},
            onSeek: { _ in },
            onToggleRemainingTime: {},
            onVolumeDrag: { _ in },
            onVolumeChange: { _ in },
            onSetMuted: { _ in },
            onSetShuffle: { _ in },
            onSetQueuePresented: { _ in },
            onSetRepeatMode: { _ in },
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
