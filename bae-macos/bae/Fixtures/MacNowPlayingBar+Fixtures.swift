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
    /// The receiver playback is on when it takes no volume from bae.
    var receiverWithoutVolume: String?

    @State
    private var showQueue = false
    @State
    private var volume: Float = 0.7
    @State
    private var isMuted = false

    var body: some View {
        MacNowPlayingBar(
            trackTitle: trackTitle,
            secondaryLine: artistNames,
            cover: nil,
            isPlaying: isPlaying,
            position: hasPosition
                ? PreviewData.trackPosition(
                    positionMs: Int64(progress * 222_000),
                    durationMs: 222_000
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
            storageWait:
                LatestStream.trackStorageWaits(
                    waiting: waitsOnStorage ? ["preview-track"] : []
                )
                .source(for: "preview-track"),
            showRemainingTime: false,
            settings: PlaybackSettings(
                volume: volume,
                isMuted: isMuted,
                repeatMode: repeatMode
            ),
            receiverWithoutVolume: receiverWithoutVolume,
            shuffled: shuffled,
            showQueue: showQueue,
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
            castControl: AnyView(EmptyView()),
        )
        .frame(width: 1100)
    }
}
