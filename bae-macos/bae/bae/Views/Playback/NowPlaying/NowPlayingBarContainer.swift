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
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.coverImage
            .map {
                .libraryImage($0)
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
            showQueue: uiStore.showQueue,
            onPlayPause: { playback.playPause(for: playbackStore.nowPlaying) },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { ratio in
                playbackStore.projectSeek(ratio: ratio)
                playback.seekByRatio(ratio)
            },
            onToggleRemainingTime: {
                // Write-through: the config subscription re-renders the
                // bar, so there is nothing to flip locally.
                try? playback.setShowRemainingTime(
                    !configStore.config.showRemainingTime
                )
            },
            onVolumeChange: { playback.setVolume($0) },
            onToggleMute: { playback.setMuted(!playbackStore.isMuted) },
            onSetShuffle: { queue.setShuffle($0) },
            onSetQueuePresented: { uiStore.setQueuePresented($0) },
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
            queueAddPublisher: playbackStore.queueItemsAddedPublisher,
            // Casting is opt-in, and while it is off core browses nothing and
            // refuses every session — so the bar simply has no Cast control.
            castControl: configStore.config.castEnabled
                ? AnyView(CastButton()) : AnyView(EmptyView()),
        )
    }
}

#if DEBUG
    // MARK: - Previews

    /// Drives the container with the same environment the app wires: the
    /// services as stubs, a config store, and a `PlaybackStore` pre-seeded with
    /// a now-playing track and a queue so the bar renders a full playing state.
    #Preview("Playing") {
        let store = PreviewData.queueStore(manualCount: 2)
        store.play(
            track: NowPlayingTrack(
                trackId: "t-np",
                trackTitle: PreviewData.nowPlayingTitle,
                artistNames: PreviewData.nowPlayingArtist,
                albumId: "a-01",
                coverImage: nil,
                durationMs: 222_000,
            )
        )
        return NowPlayingBarContainer(onDropToQueue: { _ in })
            .frame(width: 1100)
            .background(Theme.background)
            .environment(Playback.stub())
            .environment(Queue.stub())
            .environment(store)
            .environment(UiStore())
            .environment(PreviewData.configStore())
            .environment(ImageStore.stub())
            .environment(Cast.stub())
            .environment(CastStore())
    }
#endif
