import BaeKit
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
        .frame(width: 350)
        .frame(maxHeight: 500)
        // The panel paints the chrome the popover window used to supply:
        // frosted system material (which lifts it off the equally-dark window
        // behind), a hairline edge, and a deep shadow.
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(.white.opacity(0.16), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.5), radius: 30, y: 10)
    }
}
