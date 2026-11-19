import AppKit
import BaeKit
import Combine
import SwiftUI

/// A track row's lead, before the title: the track's number at rest, Play
/// under the pointer (Pause on the current track while it plays), and the
/// speaker, in the accent colour, on the current track.
struct MacTrackLead: View {
    let text: String
    let isCurrent: Bool
    let isPlaying: Bool
    /// Whether the pointer is over the row.
    let hover: Bool
    /// The lead's width, which the track list sets to the widest number.
    let width: CGFloat
    let onPlay: () -> Void
    let onTogglePlayPause: () -> Void

    /// A track's position as the track list draws it, on one line: the list
    /// measures every one to size the leads' number column.
    static func number(_ text: String) -> some View {
        Text(text)
            .themeText(.detail)
            .monospacedDigit()
            .lineLimit(1)
            .fixedSize()
    }

    var body: some View {
        let isCurrentPlaying = isCurrent && isPlaying
        // Every state stays in the layout, opacity-toggled, so the row's size
        // never changes.
        ZStack(alignment: .trailing) {
            Self.number(text)
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
        .frame(width: width)
    }
}
