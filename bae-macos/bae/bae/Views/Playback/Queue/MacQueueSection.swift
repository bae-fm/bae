import BaeKit
import SwiftUI

/// A list's heading in the side pane: a queue lane's title as an eyebrow,
/// then Clear and, on the lane that has one, the shuffle toggle; or a day of
/// History, the first ending in whose plays History shows (`menu`).
struct MacQueueSection: View {
    /// `nil` hides the label, for when only one list is visible.
    let title: String?
    var shuffled = false
    var count = 0
    /// What Clear empties, named for the tooltip and VoiceOver.
    var clearLabel = ""
    var onClear: () -> Void = {}
    /// Sets the lane's shuffle; `nil` on the manual lane, which has none.
    var onSetShuffle: ((Bool) -> Void)?
    /// A choice that applies to the whole list, at the heading's end.
    var menu: MacInlineMenu?
    /// More room at the end, over rows that end in a person (History).
    var wide = false
    /// Under a list whose rows hang their leading column in the margin, the
    /// heading starts at the rows' covers.
    var hang = false

    /// The inset of the lane's rows.
    private static let rowInset = ThemeSpace.related

    @ViewBuilder
    var body: some View {
        if title != nil || onSetShuffle != nil || count > 0 {
            HStack(spacing: ThemeSpace.compact) {
                if let title {
                    Eyebrow(verbatim: title)
                        .lineLimit(1)
                }
                Spacer()
                if count > 0 {
                    clearButton
                }
                if let onSetShuffle {
                    shuffleToggle(onSetShuffle)
                }
                if let menu {
                    menu
                }
            }
            // In line with the rows' content, so Clear and shuffle sit over the
            // remove buttons; under hanging rows, at the covers' edge.
            .padding(
                .leading,
                hang ? 0 : Self.rowInset + QueueItemRow.horizontalInset
            )
            .padding(
                .trailing,
                wide
                    ? ThemeSpace.edge
                    : Self.rowInset + QueueItemRow.horizontalInset
            )
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.inline)
        }
    }

    private var clearButton: some View {
        Button(action: onClear) {
            Text("Clear")
                .themeText(.eyebrow)
                .foregroundStyle(Theme.accent)
        }
        .buttonStyle(.accentPlain)
        .help(clearLabel)
        .accessibilityLabel(clearLabel)
    }

    /// The context's shuffle toggle; the current track keeps playing.
    private func shuffleToggle(_ onSetShuffle: @escaping (Bool) -> Void)
        -> some View
    {
        Button {
            onSetShuffle(!shuffled)
        } label: {
            Image(systemName: "shuffle")
                .themeIcon(.small)
                .foregroundStyle(shuffled ? Theme.accent : .secondary)
                // The same slot as the rows' remove button, so they line up.
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .background(
                    RoundedRectangle(cornerRadius: ThemeRadius.control)
                        .fill(shuffled ? Theme.accentSoft : .clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(PressableIconButtonStyle().accentFocus())
        .help(shuffled ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(shuffled ? "Turn off shuffle" : "Shuffle")
    }
}
