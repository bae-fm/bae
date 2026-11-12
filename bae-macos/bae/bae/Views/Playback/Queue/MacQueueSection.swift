import BaeKit
import SwiftUI

/// A queue lane's heading: its title as an eyebrow, then Clear and, on the
/// lane that has one, the shuffle toggle.
struct MacQueueSection: View {
    /// `nil` hides the label, for when only one list is visible.
    let title: String?
    let shuffled: Bool
    let count: Int
    /// What Clear empties, named for the tooltip and VoiceOver.
    let clearLabel: String
    let onClear: () -> Void
    /// Sets the lane's shuffle; `nil` on the manual lane, which has none.
    let onSetShuffle: ((Bool) -> Void)?

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
            }
            // In line with the rows' content, so Clear and shuffle sit over the
            // remove buttons.
            .padding(
                .horizontal,
                Self.rowInset + QueueItemRow.horizontalInset
            )
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.inline)
        }
    }

    private var clearButton: some View {
        Button(action: onClear) {
            Eyebrow("Clear")
        }
        .buttonStyle(.plain)
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
        .buttonStyle(PressableIconButtonStyle())
        .help(shuffled ? "Turn off shuffle" : "Shuffle")
        .accessibilityLabel(shuffled ? "Turn off shuffle" : "Shuffle")
    }
}
