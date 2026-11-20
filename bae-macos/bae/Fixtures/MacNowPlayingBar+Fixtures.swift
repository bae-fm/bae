import BaeKit
import Combine
import Foundation
import SwiftUI

@testable import bae

// MARK: - Shot staging

/// Preview host that holds the queue, volume, and mute state.
struct NowPlayingBarPreview: View {
    let trackTitle: String?
    let artistNames: String?
    /// The cover of the playing track, if the track has one.
    var cover: BridgeImageRef?
    let isPlaying: Bool
    /// The parts of the track whose bytes are here.
    var held = [BridgeTrackSpan(start: 0, end: 1)]
    /// Whether the track's load waits on storage.
    var waitsOnStorage = false
    let repeatMode: BridgeRepeatMode
    let shuffled: Bool
    /// The seek bar's position: how far through a track, or none.
    var hasPosition = true
    var progress = 0.25
    /// The receiver playback is on, when it is on one.
    var casting: BridgeCastReceiver?
    /// Whether the queue is open beside the bar.
    var queueOpen = false

    @State
    private var showQueue = false
    @State
    private var volume: Float = 0.7
    @State
    private var isMuted = false

    private var castStore: CastStore {
        let store = CastStore()
        if let casting {
            store.applyStatus(.on(device: casting))
        }
        return store
    }

    var body: some View {
        let castStore = castStore
        return MacNowPlayingBar(
            trackTitle: trackTitle,
            secondaryLine: artistNames,
            cover: cover.map { .libraryImage($0) },
            isPlaying: isPlaying,
            position: hasPosition
                ? PreviewData.trackPosition(
                    positionMs: Int64(progress * 303_000),
                    durationMs: 303_000
                )
                : nil,
            buffered: hasPosition
                ? LatestStream.trackBuffered(
                    holding: [
                        "preview-track": BridgeTrackBuffered(ranges: held)
                    ]
                )
                .source(for: "preview-track")
                : nil,
            waitsOnStorage: waitsOnStorage,
            showRemainingTime: false,
            settings: PlaybackSettings(
                volume: volume,
                isMuted: isMuted,
                repeatMode: repeatMode
            ),
            receiverWithoutVolume: castStore.receiverWithoutVolume,
            shuffled: shuffled,
            showQueue: showQueue || queueOpen,
            onPlayPause: {},
            onNext: {},
            onPrevious: {},
            onSeek: { _ in },
            onToggleRemainingTime: {},
            onVolumeDrag: { volume = $0 },
            onVolumeChange: { volume = $0 },
            onSetMuted: { isMuted = $0 },
            onSetShuffle: { _ in },
            onSetQueuePresented: { showQueue = $0 },
            onSetRepeatMode: { _ in },
            onDropToQueue: { _ in },
            onNavigateToAlbum: {},
            queueAddPublisher: Empty().eraseToAnyPublisher(),
            castControl: AnyView(CastButton()),
        )
        .environment(Cast.stub())
        .environment(CastStart.stub())
        .environment(castStore)
        .frame(width: 1100)
    }
}
