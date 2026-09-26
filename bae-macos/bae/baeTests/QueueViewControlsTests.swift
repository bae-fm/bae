import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

@Suite("Queue pane controls")
struct QueueViewControlsTests {
    @MainActor
    @Test("the pane exposes a Close control")
    func paneExposesCloseControl() async throws {
        let recorder = Recorder()
        try await withHostedPane(nowPlaying: .stopped, recorder: recorder) {
            hosted in
            try click(
                atTopOrigin: closeControlPoint,
                in: hosted.view,
                window: hosted.window
            )
            try await SnapshotTestSupport.settle(hosted.view)
        }

        #expect(recorder.closeCount == 1)
    }

    @MainActor
    @Test("the playing card exposes Pause")
    func playingCardExposesPause() async throws {
        let recorder = Recorder()
        try await withHostedPane(
            nowPlaying: .playing(track),
            recorder: recorder
        ) { hosted in
            try clickPlayPause(in: hosted)
            try await SnapshotTestSupport.settle(hosted.view)
        }

        #expect(recorder.playPauseCount == 1)
        #expect(recorder.navigateCount == 0)
    }

    @MainActor
    @Test("the paused card exposes Play")
    func pausedCardExposesPlay() async throws {
        let recorder = Recorder()
        try await withHostedPane(
            nowPlaying: .paused(track, reason: .manual),
            recorder: recorder
        ) { hosted in
            try clickPlayPause(in: hosted)
            try await SnapshotTestSupport.settle(hosted.view)
        }

        #expect(recorder.playPauseCount == 1)
        #expect(recorder.navigateCount == 0)
    }

    /// The queue pane in `nowPlaying`, hosted and settled for the length of
    /// `body`.
    @MainActor
    private func withHostedPane(
        nowPlaying: NowPlaying,
        recorder: Recorder,
        _ body: ((window: NSWindow, view: NSView)) async throws -> Void
    ) async throws {
        let store = PreviewData.queueStore(manualCount: 0, context: nil)
        switch nowPlaying {
        case .playing(let track):
            store.play(track: track)
        case .paused(let track, let reason):
            store.pause(track: track, reason: reason)
        case .stopped:
            break
        case .loading:
            Issue.record("the control fixture does not use a loading state")
        }
        try await SnapshotTestSupport.withHostedWindow(
            QueueView(
                isActive: store.nowPlaying.isActive,
                nowPlayingTitle: store.nowPlaying.track?.trackTitle,
                nowPlayingArtist: store.nowPlaying.track?.artistNames,
                nowPlayingCover: nil,
                isPlaying: store.nowPlaying.isPlaying,
                isLoading: store.nowPlaying.loadingTrackId != nil,
                onClose: { recorder.closeCount += 1 },
                onGoToNowPlaying: { recorder.navigateCount += 1 },
                onPlayPause: { recorder.playPauseCount += 1 },
                onClearUpNext: {},
                onClearPlayingFrom: {},
                onSkipTo: { _ in },
                onRemove: { _ in },
                onReorder: { _, _ in },
                onInsertTracks: { _, _ in },
                onSetShuffle: { _ in }
            )
            .environment(store)
            .environment(Queue.stub())
            .environment(ImageStore.stub()),
            size: paneSize
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            try await body((window: window, view: host))
        }
    }

    @MainActor
    private func clickPlayPause(
        in hosted: (window: NSWindow, view: NSView)
    ) throws {
        try click(
            atTopOrigin: playPauseControlPoint,
            in: hosted.view,
            window: hosted.window
        )
    }

    @MainActor
    private func click(
        atTopOrigin point: NSPoint,
        in host: NSView,
        window: NSWindow
    ) throws {
        try HostedInput.click(
            at: pointInWindow(atTopOrigin: point, in: host),
            in: window
        )
    }

    @MainActor
    private func pointInWindow(
        atTopOrigin point: NSPoint,
        in host: NSView
    ) -> NSPoint {
        let hostPoint = NSPoint(
            x: point.x,
            y: host.isFlipped ? point.y : host.bounds.height - point.y
        )
        return host.convert(hostPoint, to: nil)
    }

    private var paneSize: NSSize {
        NSSize(width: 420, height: 720)
    }

    private var closeControlPoint: NSPoint {
        NSPoint(x: 385, y: 33)
    }

    private var playPauseControlPoint: NSPoint {
        NSPoint(x: 379, y: 87)
    }

    private var track: NowPlayingTrack {
        NowPlayingTrack(
            trackId: "track-id",
            trackTitle: "Track Title",
            artistNames: "Artist Name",
            albumId: "album-id",
            coverImage: nil,
            durationMs: 180_000
        )
    }

    @MainActor
    private final class Recorder {
        var closeCount = 0
        var navigateCount = 0
        var playPauseCount = 0
    }
}

@Suite("Queue presentation state")
struct QueuePresentationStateTests {
    @Test("setting presentation is absolute and idempotent")
    func presentationCommandIsAbsolute() {
        let store = UiStore()

        store.setQueuePresented(true)
        store.setQueuePresented(true)
        #expect(store.showQueue)

        store.setQueuePresented(false)
        store.setQueuePresented(false)
        #expect(!store.showQueue)
    }
}
