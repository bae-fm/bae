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
    @Environment(PlaybackStore.self)
    private var playbackStore
    @Environment(Queue.self)
    private var queue

    let onInsertTracks: ([String], Int) -> Void

    var body: some View {
        let np = playbackStore.nowPlaying
        let track = np.track
        let cover: ImageContent? =
            track?.coverImageId
            .map {
                .library(.cover(id: $0, version: nil))
            }
        QueueView(
            isActive: np.isActive,
            nowPlayingTitle: track?.trackTitle,
            nowPlayingArtist: track?.artistNames,
            nowPlayingCover: cover,
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
        // Sidebar chrome: an elevated-surface gradient that lifts it off the
        // darker content beside it, and a hairline on the docked edge.
        .background(
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            )
        )
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(.white.opacity(0.16))
                .frame(width: 1)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    /// Previews render the panel — the queue's root, chrome included — with the
    /// same environment wiring production uses: now-playing and the lanes come
    /// off the `PlaybackStore`, and commands go to the echoing preview `Queue`,
    /// whose snapshot re-application stands in for core's `QueueUpdated` echo —
    /// without it, a committed drag's display order never reconciles and the
    /// next drag misbehaves.
    @MainActor
    private func queuePanelPreview(
        store: PlaybackStore,
        queue: Queue
    ) -> some View {
        QueuePanel(onInsertTracks: { ids, index in
            queue.insertInQueue(ids, UInt32(index))
        })
        .environment(store)
        .environment(queue)
        .environment(MediaPaths.stub)
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
                coverImageId: nil,
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
