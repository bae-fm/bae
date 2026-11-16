import AppKit
import BaeKit
import SwiftUI

/// One row in an album's track list, as drawn: the position, the title and
/// artist, the duration, the hover fill and the flash that marks the track a
/// jump arrived at. `TrackRowView` owns what moves them.
struct MacTrackRow: View {
    let track: Track
    /// The artist to show, or `nil` for none — core's decision.
    let artist: String?
    /// The widest position label in the release, so every row's number
    /// column is as wide and no label wraps.
    let positionWidth: CGFloat
    let isCurrent: Bool
    let isPlaying: Bool
    /// Whether the pointer is over the row.
    var hover = false
    /// Whether the row is among the tracks picked together.
    var picked = false
    /// How much of the accent the jump's flash fills the row with.
    var highlight: Double = 0
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void

    /// The leading slot's least width: the play button and the playing
    /// glyph that take the number's place.
    static let leadingSlotMinWidth: CGFloat = 22

    var body: some View {
        let isCurrentPlaying = isCurrent && isPlaying
        HStack(spacing: ThemeSpace.group) {
            // Every leading-slot state stays in the layout, opacity-toggled,
            // so the row's size never changes.
            ZStack(alignment: .trailing) {
                TrackPositionLabel(text: track.positionText)
                    .foregroundStyle(.tertiary)
                    .opacity(!isCurrent && !hover ? 1 : 0)

                Button(action: isCurrent ? onTogglePlayPause : onPlay) {
                    Image(
                        systemName: isCurrentPlaying
                            ? "pause.fill" : "play.fill"
                    )
                    .themeIcon(.medium)
                }
                .buttonStyle(.accentPlain)
                .opacity(hover ? 1 : 0)
                .allowsHitTesting(hover)

                Image(systemName: "speaker.wave.2.fill")
                    .themeIcon(.small)
                    .foregroundStyle(Theme.accent)
                    .opacity(isCurrent && !hover ? 1 : 0)
                    .accessibilityLabel("Now playing")
                    .accessibilityHidden(!isCurrent)
            }
            .frame(width: max(positionWidth, Self.leadingSlotMinWidth))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(track.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(isCurrent ? Theme.accent : .primary)
                    .lineLimit(1)
                if let artist {
                    Text(artist)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            Text(track.durationLabel)
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, ThemeSpace.related)
        .frame(maxHeight: .infinity)
        .background(
            ZStack {
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(
                        picked
                            ? Theme.accentSoft
                            : hover ? Theme.hover : Color.clear
                    )
                RoundedRectangle(cornerRadius: ThemeRadius.control)
                    .fill(Theme.accent.opacity(highlight))
            }
        )
        // The hover fill bleeds past the text column so the text stays aligned
        // with the header above the list.
        .padding(.horizontal, -ThemeSpace.related)
    }
}

/// One row in an album's track list: a `MacTrackRow` that follows the
/// pointer, flashes when a jump arrives at its track, plays on a double click
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
    /// Rate the track with these stars, or clear its rating with none; `nil`
    /// for a person who can't rate, whose menu has no Rate.
    var onRate: ((BridgeStars?) -> Void)?
    /// The playlists the track's menu can add it to.
    var playlists = TrackPlaylistChoices.none
    /// What a menu on several picked tracks does, or `nil` where the list
    /// picks no tracks. The picks themselves are the page's, in `UiStore`.
    var picking: TrackPicking?

    @State
    private var isHovered = false
    @State
    private var hoverWorkItem: DispatchWorkItem?
    @State
    private var highlightOpacity: Double = 0

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            MacTrackRow(
                track: track,
                artist: artist,
                positionWidth: positionWidth,
                isCurrent: isCurrent,
                isPlaying: isPlaying,
                hover: isHovered,
                picked: isPicked,
                highlight: highlightOpacity,
                onPlay: onPlay,
                onTogglePlayPause: onTogglePlayPause
            )
            Stars(value: track.myStars?.number ?? 0, size: 10)
                .opacity(track.myStars == nil ? 0 : 1)
                .accessibilityHidden(track.myStars == nil)
        }
        .revealsAsTrackRow(track.id)
        // Keyed on the flash's `seq` so a remounted row still sees the flash
        // and a repeat navigation fires it again.
        .task(id: uiStore.pendingTrackFlash?.seq) {
            guard let flash = uiStore.pendingTrackFlash,
                flash.trackId == track.id
            else {
                return
            }
            highlightOpacity = ThemeOpacity.tintStrong
            withAnimation(.easeOut(duration: 3)) {
                highlightOpacity = 0
            }
            uiStore.consumeTrackFlash(seq: flash.seq)
        }
        .onTapGesture(count: 2) {
            onPlay()
        }
        .simultaneousGesture(
            TapGesture().onEnded { handleTap() }
        )
        .accessibilityAddTraits(isPicked ? .isSelected : [])
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

    private var isPicked: Bool {
        picking != nil && uiStore.pickedTracks.contains(track.id)
    }

    /// Cmd picks or unpicks the row, shift picks the range from the last
    /// pick, and a plain click clears the picks.
    private func handleTap() {
        guard let picking else { return }
        let picked = uiStore.pickedTracks
        let modifiers = NSEvent.modifierFlags
        if modifiers.contains(.command) {
            picked.set(track.id, picked: !isPicked)
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
            onRate: onRate,
            playlists: playlists
        )
    }
}

/// A track's position as the track list draws it, on one line: the rows show
/// it, and the list measures every one to size the rows' number column.
struct TrackPositionLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .themeText(.detail)
            .monospacedDigit()
            .lineLimit(1)
            .fixedSize()
    }
}
