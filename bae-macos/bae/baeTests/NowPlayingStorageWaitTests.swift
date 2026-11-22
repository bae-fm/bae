import AppKit
import BaeKit
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
        let held = [BridgeTrackSpan(start: 0, end: 0.08)]
        let size = NSSize(width: 1_100, height: 120)
        try await SnapshotTestSupport.withHostedWindow(
            bar(held: held, waitsOnStorage: true)
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
            bar(held: [], waitsOnStorage: false)
                .frame(width: size.width, height: size.height),
            size: size
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            let arcs = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? StorageWaitArcNSView }
            #expect(arcs.allSatisfy { $0.isHidden }, "drew \(arcs)")
        }
    }

    private func bar(
        held: [BridgeTrackSpan],
        waitsOnStorage: Bool
    ) -> some View {
        NowPlayingBarFixture.make(
            settings: PlaybackSettings(
                volume: 0.7,
                isMuted: false,
                repeatMode: .off
            ),
            track: NowPlayingBarFixture.Track(
                positionMs: 0,
                durationMs: 200_000,
                held: held,
                waitsOnStorage: waitsOnStorage
            )
        )
        .environment(ImageStore.stub())
        .environment(PreviewData.queueStore(manualCount: 0, context: nil))
        .environment(Queue.stub())
    }
}
