import BaeKit
import Combine
import SwiftUI

/// The queue, docked as a fixed-width sidebar between the title bar and the
/// now-playing bar. Docked rather than floating so the window's content
/// reflows beside it — nothing is ever occluded while it's open, which is
/// what lets album cards and track rows be dragged into its drop sites.
/// Toggled by the queue button or the menu; clicks elsewhere in the window
/// keep working and keep the panel open.
struct QueuePanel: View {
    @Environment(Playback.self)
    private var playback
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    @Environment(LibraryStore.self)
    private var libraryStore
    @Environment(UiStore.self)
    private var uiStore

    let onClose: () -> Void
    let onInsertTracks: ([String], Int) -> Void

    var body: some View {
        let navigation = NowPlayingNavigationAction(
            playbackStore: playbackStore,
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        let onGoToNowPlaying: (() -> Void)? =
            navigation.isEnabled ? { navigation.perform() } : nil
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.coverImage
            .map {
                .libraryImage($0)
            }
        QueueView(
            isActive: np.isActive,
            nowPlayingTitle: track?.trackTitle,
            nowPlayingArtist: track?.artistNames,
            nowPlayingCover: cover,
            isPlaying: np.isPlaying,
            isLoading: np.loadingTrackId != nil,
            onClose: onClose,
            onGoToNowPlaying: onGoToNowPlaying,
            onPlayPause: {
                playback.playPause(for: playbackStore.nowPlaying)
            },
            onClearUpNext: { queue.clearUpNext() },
            onClearPlayingFrom: { queue.clearPlayingFrom() },
            onSkipTo: { queue.skipToEntry($0) },
            onRemove: { queue.removeEntry($0) },
            onReorder: { entryId, beforeEntryId in
                queue.reorderEntry(entryId, beforeEntryId)
            },
            onInsertTracks: onInsertTracks,
            onSetShuffle: { queue.setShuffle($0) },
        )
        .frame(width: 420)
        .frame(maxHeight: .infinity)
        // Sidebar chrome: one surface step above the content beside it, and
        // a hairline on the docked edge.
        .background(Theme.surface)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Theme.hairline)
                .frame(width: 1)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Previews render the panel — the queue's root, chrome included — with the
    /// same environment wiring production uses: now-playing and the lanes come
    /// off the `PlaybackStore`, and commands go to the echoing preview `Queue`,
    /// whose snapshot re-application stands in for core's queue value delivery —
    /// without it, a committed drag's display order never reconciles and the
    /// next drag misbehaves.
    @MainActor
    private func queuePanelPreview(
        store: PlaybackStore,
        queue: Queue
    ) -> some View {
        QueuePanel(
            onClose: {},
            onInsertTracks: { ids, index in
                queue.insertInQueue(ids, UInt32(index))
            }
        )
        .environment(Playback.stub())
        .environment(store)
        .environment(queue)
        .environment(ImageStore.stub())
        .environment(LibraryStore())
        .environment(UiStore())
        .environment(
            \.playbackPositionPublisher,
            Just(
                PlaybackPositionEvent.position(
                    progress: 0.34,
                    positionMs: 73_000,
                    durationMs: 214_000
                )
            )
            .eraseToAnyPublisher()
        )
        // The preview canvas proposes ideal size, under which the panel's
        // ScrollView collapses; a firm height stands in for the window slot
        // the sidebar fills in the app.
        .frame(height: 720)
        .background(Theme.background)
    }

    #Preview("With items") {
        let (store, queue) = PreviewData.echoingQueue(
            manualCount: 2,
            shuffled: true
        )
        store.play(
            track: NowPlayingTrack(
                trackId: "t-np",
                trackTitle: PreviewData.nowPlayingTitle,
                artistNames: PreviewData.nowPlayingArtist,
                albumId: "a-01",
                coverImage: nil,
                durationMs: 214_000
            )
        )
        return queuePanelPreview(store: store, queue: queue)
    }

    #Preview("Empty") {
        let (store, queue) = PreviewData.echoingQueue(
            manualCount: 0,
            context: false
        )
        return queuePanelPreview(store: store, queue: queue)
    }
#endif
