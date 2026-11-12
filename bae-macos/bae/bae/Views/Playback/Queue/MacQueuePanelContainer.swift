import BaeKit
import SwiftUI

/// Reads the stores for `MacQueuePanel`: now-playing and the lanes come off the
/// `PlaybackStore`, and every command goes to `Queue` and `Playback`.
struct MacQueuePanelContainer: View {
    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    @Environment(UiStore.self)
    private var uiStore

    let onClose: () -> Void
    let onInsertTracks: ([String], Int) -> Void

    var body: some View {
        let navigation = NowPlayingNavigationAction(
            playbackStore: playbackStore,
            uiStore: uiStore
        )
        let onGoToNowPlaying: (() -> Void)? =
            navigation.isEnabled ? { navigation.perform() } : nil
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.display.coverImage
            .map {
                .libraryImage($0)
            }
        MacQueuePanel(
            manual: playbackStore.manualQueue,
            manualVersion: playbackStore.manualVersion,
            context: playbackStore.queueContext,
            shuffled: playbackStore.shuffled,
            upcoming: QueueUpcoming(store: playbackStore, queue: queue),
            position: playbackStore.trackPosition(
                forTrack: track?.track.trackId
            ),
            onShowUpcoming: { playbackStore.showUpcoming(queue: queue) },
            isActive: np.isActive,
            nowPlayingTitle: track?.display.title,
            nowPlayingArtist: track?.display.artistNames,
            nowPlayingCover: cover,
            isPlaying: np.isPlaying,
            onClose: onClose,
            onGoToNowPlaying: onGoToNowPlaying,
            onPlayPause: {
                playback.playPause(for: playbackStore.nowPlaying)
            },
            onClearUpNext: {
                Task {
                    do { try await queue.clearUpNext() }
                    catch { uiStore.showError(error) }
                }
            },
            onClearPlayingFrom: {
                Task {
                    do { try await queue.clearPlayingFrom() }
                    catch { uiStore.showError(error) }
                }
            },
            onSkipTo: { entryId in
                Task {
                    do { try await queue.skipToEntry(entryId) }
                    catch { uiStore.showError(error) }
                }
            },
            onRemove: { entryId in
                Task {
                    do { try await queue.removeEntry(entryId) }
                    catch { uiStore.showError(error) }
                }
            },
            onReorder: { entryId, beforeEntryId in
                try await queue.reorderEntry(entryId, beforeEntryId)
            },
            onReorderFailed: { uiStore.showError($0) },
            onInsertTracks: onInsertTracks,
            onSetShuffle: { queue.setShuffle($0) },
        )
    }
}
