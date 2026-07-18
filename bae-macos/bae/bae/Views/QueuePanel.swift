import BaeKit
import Combine
import SwiftUI

/// The queue, presented as a floating panel INSIDE the main window, anchored
/// above the now-playing bar's queue button. Deliberately not an `NSPopover`:
/// SwiftUI's popover runs its own dismissal monitors (an anchor click closes
/// it before the button's action even runs) and hosts content in a separate
/// window whose show/close animation stutters under main-thread work and
/// ignores presenter-side animation entirely. In-tree, the entrance/exit
/// springs, dismissal routing, and drag sessions are all ours.
///
/// Dismissal matches the old `.applicationDefined` popover: the queue button,
/// the close control, or the menu toggle — clicks elsewhere in the window keep
/// working and keep the panel open (transport stays usable while the queue is
/// up).
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
            onClear: { queue.clearQueue() },
            onSkipTo: { queue.skipToEntry($0) },
            onRemove: { queue.removeEntry($0) },
            onReorder: { entryId, beforeEntryId in
                queue.reorderEntry(entryId, beforeEntryId)
            },
            onInsertTracks: onInsertTracks,
            onSetShuffle: { queue.setShuffle($0) },
        )
        .frame(width: 420)
        .frame(maxHeight: 560)
        // The panel paints the chrome the popover window used to supply: an
        // elevated-surface gradient (which lifts it off the darker window
        // behind), a hairline edge, and a deep shadow.
        .background(
            LinearGradient(
                colors: [Theme.surfaceElevated, Theme.surface],
                startPoint: .top,
                endPoint: .bottom
            ),
            in: RoundedRectangle(cornerRadius: 18)
        )
        .clipShape(RoundedRectangle(cornerRadius: 18))
        .overlay(
            RoundedRectangle(cornerRadius: 18)
                .stroke(.white.opacity(0.16), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.5), radius: 30, y: 10)
    }
}

#if DEBUG
    // MARK: - Previews

    /// Previews render the panel — the queue's root, chrome included — with the
    /// same environment wiring production uses: now-playing and the lanes come
    /// off the `PlaybackStore`, commands go to the stub `Queue`.
    @MainActor
    private func queuePanelPreview(store: PlaybackStore) -> some View {
        QueuePanel(onInsertTracks: { _, _ in })
            .environment(store)
            .environment(Queue.stub)
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
            .padding(40)
            .background(Theme.background)
    }

    #Preview("With items") {
        let store = PreviewData.queueStore(manualCount: 2, shuffled: true)
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
        return queuePanelPreview(store: store)
    }

    #Preview("Empty") {
        queuePanelPreview(
            store: PreviewData.queueStore(manualCount: 0, context: nil)
        )
    }
#endif
