import AppKit
import BaeKit
import Combine
import SwiftUI

/// One row in an album's track list, as drawn: the position, the title and
/// artist, the duration, the hover fill and the flash that marks the track a
/// jump arrived at, and, in a list that keeps one, the slot for the track's
/// stars after it. `TrackRowView` owns what moves them.
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
    /// The slot after the row for the stars the person gave the track, which
    /// an album's list keeps so every row's duration lines up; `nil` for a
    /// row with no slot.
    var rating: Rating?
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void

    /// The leading slot's least width: the play button and the playing
    /// glyph that take the number's place.
    static let leadingSlotMinWidth: CGFloat = 22

    /// The stars' slot: the stars the person gave the track, or `nil` for a
    /// track they did not rate, which leaves the slot empty.
    struct Rating {
        let stars: BridgeStars?
    }

    var body: some View {
        if let rating {
            HStack(spacing: ThemeSpace.compact) {
                row
                Stars(value: rating.stars?.number ?? 0, size: 10)
                    .opacity(rating.stars == nil ? 0 : 1)
                    .accessibilityHidden(rating.stars == nil)
            }
        }
        else {
            row
        }
    }

    private var row: some View {
        HStack(spacing: ThemeSpace.group) {
            MacTrackLead(
                text: track.positionText,
                isCurrent: isCurrent,
                isPlaying: isPlaying,
                hover: hover,
                width: max(positionWidth, Self.leadingSlotMinWidth),
                onPlay: onPlay,
                onTogglePlayPause: onTogglePlayPause
            )
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
