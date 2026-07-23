import BaeKit
import Combine
import SwiftUI

/// Isolates observation so that only slow-changing properties (track info, volume, etc.)
/// trigger SwiftUI re-evaluation. Position ticks bypass SwiftUI entirely via the AppKit
/// SeekBarNSView, which AppService updates directly.
struct NowPlayingBarContainer: View {
    @Environment(Playback.self)
    var playback
    @Environment(Queue.self)
    var queue
    @Environment(PlaybackStore.self)
    var playbackStore
    @Environment(UiStore.self)
    var uiStore
    @Environment(ConfigStore.self)
    var configStore
    let onDropToQueue: ([String]) -> Void

    var body: some View {
        @Bindable
        var uiStore = uiStore
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.coverImageId
            .map {
                .library(.cover(id: $0, version: nil))
            }
        NowPlayingBar(
            trackTitle: track?.trackTitle,
            secondaryLine: np.secondaryLine,
            cover: cover,
            isPlaying: np.isPlaying,
            isLoading: np.loadingTrackId != nil,
            durationMs: track?.durationMs,
            showRemainingTime: configStore.config.showRemainingTime,
            volume: playbackStore.volume,
            isMuted: playbackStore.isMuted,
            repeatMode: playbackStore.repeatMode,
            // nil while no playing context — the bar disables shuffle then.
            shuffled: playbackStore.queueContext?.shuffled,
            showQueue: $uiStore.showQueue,
            onPlayPause: { playback.playPause(for: playbackStore.nowPlaying) },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { ratio in
                playbackStore.projectSeek(ratio: ratio)
                playback.seekByRatio(ratio)
            },
            onToggleRemainingTime: {
                // Write-through: the config invalidation is what re-renders the
                // bar, so there is nothing to flip locally.
                try? playback.setShowRemainingTime(
                    !configStore.config.showRemainingTime
                )
            },
            onVolumeChange: { playback.setVolume($0) },
            onToggleMute: { playback.setMuted(!playbackStore.isMuted) },
            onSetShuffle: { queue.setShuffle($0) },
            onCycleRepeat: {
                playback.setRepeatMode(
                    bridgeNextRepeatMode(mode: playbackStore.repeatMode)
                )
            },
            onDropToQueue: onDropToQueue,
            onNavigateToAlbum: {
                if let albumId = track?.albumId {
                    uiStore.navigateToAlbum(albumId)
                }
            },
            queueAddPublisher: playbackStore.queueItemsAddedSubject
                .eraseToAnyPublisher(),
            castControl: AnyView(CastButton()),
        )
        .sidePausePromptAlert()
    }
}
