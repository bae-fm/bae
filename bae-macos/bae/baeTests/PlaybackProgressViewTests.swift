import AppKit
import BaeKit
import Combine
import SwiftUI
import Testing

@testable import bae

@Suite("Playback timeline", .serialized)
@MainActor
struct PlaybackProgressViewTests {
    @Test("position updates display both clocks after a reset")
    func positionAfterReset() async throws {
        let events = CurrentValueSubject<PlaybackPositionEvent, Never>(.reset)
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                showRemainingTime: false,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            )
            .environment(
                \.playbackPositionPublisher,
                events.eraseToAnyPublisher()
            ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            await drainPositionUpdates()

            events.send(
                .position(
                    progress: 0.25,
                    positionMs: 45_000,
                    durationMs: 180_000
                )
            )
            await drainPositionUpdates()

            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(slider.doubleValue == 0.25)
            #expect(labels.map(\.stringValue) == ["0:45", "3:00"])
        }
    }

    @Test(
        "a newly mounted bar replays complete clocks",
        arguments: [false, true]
    )
    func replayAndClockModes(showRemaining: Bool) async throws {
        let events = CurrentValueSubject<PlaybackPositionEvent, Never>(
            .position(progress: 0.25, positionMs: 25_000, durationMs: 100_000)
        )
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                showRemainingTime: showRemaining,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            )
            .environment(
                \.playbackPositionPublisher,
                events.eraseToAnyPublisher()
            ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            await drainPositionUpdates()
            let labels = SnapshotTestSupport.descendants(of: host)
                .compactMap { $0 as? NSTextField }
            #expect(
                labels.map(\.stringValue) == [
                    showRemaining ? "-1:15" : "0:25", "1:40",
                ]
            )

            events.send(
                .position(progress: 0, positionMs: -1_250, durationMs: 100_000)
            )
            await drainPositionUpdates()
            #expect(labels.map(\.stringValue) == ["-0:02", "1:40"])

            events.send(
                .position(progress: 0, positionMs: 25_000, durationMs: 0)
            )
            await drainPositionUpdates()
            #expect(labels.map(\.stringValue) == ["0:25", ""])
        }
    }

    @Test("preference refreshes preserve the timeline and cannot undo a reset")
    func preferenceRefresh() async throws {
        let events = CurrentValueSubject<PlaybackPositionEvent, Never>(
            .position(progress: 0.25, positionMs: 45_000, durationMs: 180_000)
        )
        func content(showRemaining: Bool) -> some View {
            PlaybackProgressView(
                showRemainingTime: showRemaining,
                onSeek: { _ in },
                onToggleRemainingTime: {}
            )
            .environment(
                \.playbackPositionPublisher,
                events.eraseToAnyPublisher()
            )
        }
        try await SnapshotTestSupport.withHostedWindow(
            content(showRemaining: false),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            await drainPositionUpdates()
            let descendants = SnapshotTestSupport.descendants(of: host)
            let bar = try #require(
                descendants.compactMap { $0 as? SeekBarNSView }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(labels.map(\.stringValue) == ["0:45", "3:00"])

            host.rootView = content(showRemaining: true)
            try await SnapshotTestSupport.settle(host)
            #expect(labels.map(\.stringValue) == ["-2:15", "3:00"])
            #expect(
                SnapshotTestSupport.descendants(of: host)
                    .contains { $0 === bar }
            )

            events.send(.reset)
            await drainPositionUpdates()
            host.rootView = content(showRemaining: false)
            try await SnapshotTestSupport.settle(host)
            #expect(labels.map(\.stringValue) == ["", ""])

            events.send(
                .position(
                    progress: 0.5,
                    positionMs: 120_000,
                    durationMs: 240_000
                )
            )
            await drainPositionUpdates()
            #expect(labels.map(\.stringValue) == ["2:00", "4:00"])
        }
    }

    @Test("seeking publishes the dropped position with both clocks")
    func seekUpdatesTimeline() async throws {
        let store = PlaybackStore()
        store.play(
            track: NowPlayingTrack(
                trackId: "track",
                trackTitle: "Track Title",
                artistNames: "Artist Name",
                albumId: "album",
                coverImage: nil,
                durationMs: 180_000
            )
        )
        _ = store.updatePlaybackProgress(
            trackId: "track",
            positionMs: 45_000,
            durationMs: 180_000,
            progress: 0.25
        )
        var seeks: [Double] = []
        try await SnapshotTestSupport.withHostedWindow(
            PlaybackProgressView(
                showRemainingTime: false,
                onSeek: { ratio in
                    seeks.append(ratio)
                    store.projectSeek(ratio: ratio)
                },
                onToggleRemainingTime: {}
            )
            .environment(
                \.playbackPositionPublisher,
                store.playbackPositionPublisher
            ),
            size: NSSize(width: 460, height: 40)
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            await drainPositionUpdates()
            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            try clickMiddle(of: slider, in: window)
            await drainPositionUpdates()
            #expect(seeks.count == 1)
            #expect(abs(try #require(seeks.first) - 0.5) < 0.01)
            #expect(labels.map(\.stringValue) == ["1:30", "3:00"])
        }
    }

    @Test("import audition displays both clocks after an idle update")
    func previewAfterReset() async throws {
        let store = ImportStore()
        let handler = DesktopEventHandler(importStore: store)
        let target = BridgePreviewTarget(
            path: "/Music/Track.flac",
            startSample: 0,
            endSample: nil
        )
        try await SnapshotTestSupport.withHostedWindow(
            PreviewProgressView(onSeek: { _ in })
                .environment(
                    \.previewProgressPublisher,
                    store.previewProgressSubject.eraseToAnyPublisher()
                ),
            size: NSSize(width: 460, height: 40)
        ) { _, host in
            try await SnapshotTestSupport.settle(host)
            await drainPositionUpdates()

            handler.apply(
                BridgePreviewValues(
                    state: .playing(
                        target: target,
                        durationMs: 180_000
                    ),
                    positionMs: 45_000,
                    progress: 0.25
                )
            )
            await drainPositionUpdates()

            let descendants = SnapshotTestSupport.descendants(of: host)
            let slider = try #require(
                descendants.compactMap { $0 as? SeekSlider }.first
            )
            let labels = descendants.compactMap { $0 as? NSTextField }
            #expect(slider.doubleValue == 0.25)
            #expect(labels.map(\.stringValue) == ["0:45", "3:00"])
            handler.apply(
                BridgePreviewValues(
                    state: .paused(
                        target: target,
                        durationMs: 240_000
                    ),
                    positionMs: 120_000,
                    progress: 0.5
                )
            )
            await drainPositionUpdates()
            #expect(slider.doubleValue == 0.5)
            #expect(labels.map(\.stringValue) == ["2:00", "4:00"])
            #expect(store.previewState.active?.isPlaying == false)
            #expect(store.previewState.active?.target == target)

            handler.apply(
                BridgePreviewValues(state: .idle, positionMs: 0, progress: 0)
            )
            await drainPositionUpdates()
            #expect(slider.doubleValue == 0)
            #expect(labels.map(\.stringValue) == ["", ""])
            #expect(store.previewState.active == nil)
        }
    }

    private func drainPositionUpdates() async {
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
    }
}

@MainActor
private func clickMiddle(of slider: SeekSlider, in window: NSWindow) throws {
    let cell = try #require(slider.cell as? NSSliderCell)
    let rect = cell.barRect(flipped: slider.isFlipped)
    let point = slider.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    try HostedInput.track(slider, at: point)
}
