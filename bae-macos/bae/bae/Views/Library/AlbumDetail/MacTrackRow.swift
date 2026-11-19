import AppKit
import BaeKit
import Combine
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
                MacTrackLead(text: track.positionText)
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
