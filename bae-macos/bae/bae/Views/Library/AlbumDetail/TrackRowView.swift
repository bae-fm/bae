import AppKit
import BaeKit
import Combine
import SwiftUI

/// One row in an album's track list: a `MacTrackRow` with its stars' slot
/// that follows the pointer, flashes when a jump arrives at its track, plays on a double click
/// and offers its context menu.
struct TrackRowView: View {
    @Environment(UiStore.self)
    private var uiStore
    let track: Track
    let artist: String?
    let positionWidth: CGFloat
    let isCurrent: Bool
    let isPlaying: Bool
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void
    /// Rate the track with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void
    /// Turn the person's heart on the track on or off, returning once the
    /// write does.
    let onSetHearted: (Bool) async -> Void
    /// The playlists the track's menu can add it to.
    var playlists = TrackPlaylistChoices.none
    /// What the menu does with several picked tracks, or `nil` where the
    /// person cannot pick tracks in this list. `UiStore` holds the page's
    /// picked tracks.
    var picking: TrackPicking?

    @State
    private var isHovered = false
    @State
    private var hoverWorkItem: DispatchWorkItem?
    @State
    private var highlightOpacity: Double = 0
    /// The heart the person chose for the track, until its write returns.
    @State
    private var pendingHeart: Bool?

    var body: some View {
        // The body reads the pick once, and the tap handler gets the value
        // the row draws, so the handler never reads the picks it writes.
        let picked = isPicked
        MacTrackRow(
            track: track,
            artist: artist,
            positionWidth: positionWidth,
            isCurrent: isCurrent,
            isPlaying: isPlaying,
            hover: isHovered,
            picked: picked,
            highlight: highlightOpacity,
            rating: shownRating,
            onPlay: onPlay,
            onTogglePlayPause: onTogglePlayPause
        )
        .revealsAsTrackRow(track.id)
        .onReceive(uiStore.trackFlashes) { trackId in
            guard trackId == track.id else { return }
            highlightOpacity = ThemeOpacity.tintStrong
            withAnimation(.easeOut(duration: 3)) {
                highlightOpacity = 0
            }
        }
        .onTapGesture(count: 2) {
            onPlay()
        }
        .simultaneousGesture(
            TapGesture().onEnded { handleTap(rowPicked: picked) }
        )
        .accessibilityAddTraits(picked ? .isSelected : [])
        .contentShape(Rectangle())
        .onHover { hovering in
            hoverWorkItem?.cancel()
            if hovering {
                let item = DispatchWorkItem { isHovered = true }
                hoverWorkItem = item
                DispatchQueue.main.asyncAfter(
                    deadline: .now() + 0.05,
                    execute: item
                )
            }
            else {
                isHovered = false
                hoverWorkItem = nil
            }
        }
        .macContextMenu {
            menu
        }
        .draggable(track.id)
    }

    /// The track's rating, with the heart the person just chose in place of
    /// the one the read holds until the write returns.
    private var shownRating: BridgeTrackRating {
        guard let pendingHeart, case .heart = track.rating else {
            return track.rating
        }
        return .heart(hearted: pendingHeart)
    }

    /// Choose the heart, unless an earlier choice is still being written.
    private func setHearted(_ hearted: Bool) {
        guard pendingHeart == nil else { return }
        pendingHeart = hearted
        Task {
            await onSetHearted(hearted)
            pendingHeart = nil
        }
    }

    private var isPicked: Bool {
        picking != nil && uiStore.pickedTracks.contains(track.id)
    }

    /// Cmd picks or unpicks the row, shift picks the range from the last
    /// pick, and a plain click clears the picks. `rowPicked` is whether the
    /// row draws itself picked.
    private func handleTap(rowPicked: Bool) {
        guard let picking else { return }
        let picked = uiStore.pickedTracks
        let modifiers = NSEvent.modifierFlags
        if modifiers.contains(.command) {
            picked.set(track.id, picked: !rowPicked)
        }
        else if modifiers.contains(.shift) {
            picking.onExtend(track.id)
        }
        else {
            picked.clear()
        }
    }

    /// The row's menu, acting on every picked track when the row is among
    /// several, else on the row alone.
    private var menu: TrackMenu {
        TrackMenu(
            track: track,
            targets: picking == nil
                ? [track.id] : uiStore.pickedTracks.targets(for: track.id),
            picking: picking,
            onPlay: onPlay,
            onAddNext: onAddNext,
            onAddToQueue: onAddToQueue,
            onExportTrack: onExportTrack,
            rating: shownRating,
            onRate: onRate,
            onSetHearted: setHearted,
            playlists: playlists
        )
    }
}
