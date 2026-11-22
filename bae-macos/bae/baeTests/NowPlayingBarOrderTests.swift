import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

/// The order the now playing bar's controls are read in, as the board lists
/// them: the track, the transport, the seek bar, then Queue, Mute and Volume.
/// Read through the accessibility API, as a screen reader reads: SwiftUI
/// builds its elements only for such a client.
@MainActor
@Suite("Now-playing bar reading order")
struct NowPlayingBarOrderTests {
    @Test(
        "the seek bar is read after the transport and before the right controls"
    )
    func seekBarBeforeRightControls() async throws {
        let size = NSSize(width: 1_200, height: 80)
        let nodes = try await SnapshotTestSupport.withHostedWindow(
            bar()
                .environment(PreviewData.artImageStore())
                .frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            return
                try AccessibilityTree.read(
                    of: window,
                    subject: "the now playing bar"
                )
                .nodes
        }
        let labels = nodes.map { $0.description ?? $0.title ?? "" }
        func index(_ words: String) throws -> Int {
            try #require(
                labels.firstIndex { $0.contains(words) },
                "listed \(labels)"
            )
        }
        let next = try index("Next")
        let seek = try index("osition")
        let queue = try index("Queue")
        let mute = try index("Mute")
        let volume = try index("Volume")
        #expect(next < seek, "listed \(labels)")
        #expect(seek < queue, "listed \(labels)")
        #expect(queue < mute, "listed \(labels)")
        #expect(mute < volume, "listed \(labels)")
    }

    private func bar() -> MacNowPlayingBar {
        MacNowPlayingBar(
            trackTitle: "Track Title",
            secondaryLine: "Artist Name",
            cover: nil,
            isPlaying: true,
            position: nil,
            buffered: nil,
            waitsOnStorage: false,
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
    }
}
