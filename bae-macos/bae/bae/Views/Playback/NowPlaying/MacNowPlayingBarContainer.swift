import BaeKit
import OSLog
import SwiftUI

private let logger = Logger.bae("MacNowPlayingBarContainer")

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
    @Environment(CastStore.self)
    var castStore
    let onDropToQueue: ([String]) -> Void

    var body: some View {
        let np = playbackStore.nowPlaying
        let library = np.display?.library
        let cover: ImageContent? =
            library?.coverImage
            .map {
                .libraryImage($0)
            }
        MacNowPlayingBar(
            trackTitle: np.display?.title,
            // The artist while the side-end card is open, which says the side
            // ended; after it closes, the line says why playback is paused.
            secondaryLine: playbackStore.presentedSidePausePrompt == nil
                ? np.secondaryLine : library?.artistLine.text,
            cover: cover,
            isPlaying: np.isPlaying,
            position: playbackStore.trackPosition(forTrack: np.trackId),
            buffered: playbackStore.trackBuffered(forTrack: np.trackId),
            waitsOnStorage: np.waitsOnStorage,
            showRemainingTime: configStore.config.showRemainingTime,
            settings: playbackStore.settings,
            receiverWithoutVolume: castStore.receiverWithoutVolume,
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
            onVolumeChange: { level in
                do {
                    try await playback.setVolume(level)
                }
                catch {
                    // Every way playback can fail to store a volume is a bug.
                    let detail = String(reflecting: error)
                    logger.error("setting the volume failed: \(detail)")
                    CoreBugReporter()
                        .report(operation: "set the volume", detail: detail)
                    uiStore.showError(error)
                }
            },
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
