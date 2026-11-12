import BaeKit
import SwiftUI

/// Reads the stores for `MacNowPlayingBar`, so the bar re-evaluates when what it
/// shows changes: a playback value (core sends one only when it changed), the
/// queue's shuffle, the queue panel, or the bar's settings. Positions never
/// re-evaluate it: it hands the seek bar the playing track's position
/// stream, which the bar's `SeekBarNSView` reads each display frame.
struct MacNowPlayingBarContainer: View {
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
            track?.display.coverImage
            .map {
                .libraryImage($0)
            }
        MacNowPlayingBar(
            trackTitle: track?.display.title,
            // The artist while the side-end card is open, which says the side
            // ended; after it closes, the line says why playback is paused.
            secondaryLine: playbackStore.presentedSidePausePrompt == nil
                ? np.secondaryLine : track?.display.artistNames,
            cover: cover,
            isPlaying: np.isPlaying,
            position: playbackStore.trackPosition(
                forTrack: track?.track.trackId
            ),
            buffered: playbackStore.trackBuffered(
                forTrack: track?.track.trackId
            ),
            storageWait: playbackStore.trackStorageWait(
                forTrack: np.loadingTrackId
            ),
            showRemainingTime: configStore.config.showRemainingTime,
            settings: playbackStore.settings,
            shuffled: playbackStore.shuffled,
            showQueue: uiStore.showQueue,
            onPlayPause: { playback.playPause(for: playbackStore.nowPlaying) },
            onNext: { playback.nextTrack() },
            onPrevious: { playback.previousTrack() },
            onSeek: { ratio in playback.seekByRatio(ratio) },
            onToggleRemainingTime: {
                // Write-through: the config subscription re-renders the
                // bar, so there is nothing to flip locally.
                let showRemaining = !configStore.config.showRemainingTime
                Task {
                    do {
                        try await playback.setShowRemainingTime(showRemaining)
                    }
                    catch {
                        uiStore.showError(error)
                    }
                }
            },
            onVolumeDrag: { playback.setVolumeLive($0) },
            onVolumeChange: { playback.setVolume($0) },
            onSetMuted: { playback.setMuted($0) },
            onSetShuffle: { queue.setShuffle($0) },
            onSetQueuePresented: { uiStore.setQueuePresented($0) },
            onSetRepeatMode: { playback.setRepeatMode($0) },
            onDropToQueue: onDropToQueue,
            onNavigateToAlbum: {
                let action = NowPlayingNavigationAction(
                    playbackStore: playbackStore,
                    uiStore: uiStore
                )
                if action.isEnabled {
                    action.perform()
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
