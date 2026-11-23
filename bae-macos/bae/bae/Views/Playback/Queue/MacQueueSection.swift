import BaeKit
import SwiftUI

/// A list's heading in the side pane: a queue lane's title as an eyebrow,
/// then its Clear and shuffle toggle; or a day of History, starting at the
/// rows' covers, the first ending in whose plays History shows.
struct MacQueueSection: View {
    /// What the heading heads, and what it offers at its end.
    enum Kind {
        case lane(Lane)
        /// A day of History; `menu`, the choice of whose plays, ends the
        /// first.
        case day(menu: MacInlineMenu?)
    }

    /// A queue lane's controls.
    struct Lane {
        /// Clear, while the lane holds tracks.
        var clear: Clear?
        /// The shuffle toggle, on the lane that has one.
        var shuffle: Shuffle?
    }

    /// Clear: what it empties, named for the tooltip and VoiceOver, and the
    /// emptying.
    struct Clear {
        let label: String
        let action: () -> Void
    }

    /// The lane's shuffle: whether it is on, and what sets it.
    struct Shuffle {
        let on: Bool
        let set: (Bool) -> Void
    }

    /// `nil` hides the label, for when only one list is visible.
    let title: String?
    let kind: Kind

    /// The inset of the lane's rows.
    private static let rowInset = ThemeSpace.related

    @ViewBuilder
    var body: some View {
        switch kind {
        case .lane(let lane):
            if title != nil || lane.clear != nil || lane.shuffle != nil {
                HStack(spacing: ThemeSpace.compact) {
                    label
                    Spacer()
                    if let clear = lane.clear {
                        clearButton(clear)
                    }
                    if let shuffle = lane.shuffle {
                        MacShuffleButton(
                            isOn: shuffle.on,
                            glyph: .small,
                            set: shuffle.set
                        )
                    }
                }
                // In line with the rows' content, so Clear and shuffle sit
                // over the remove buttons.
                .padding(
                    .horizontal,
                    Self.rowInset + MacQueueRow.horizontalInset
                )
                .padding(.top, ThemeSpace.group)
                .padding(.bottom, ThemeSpace.inline)
            }
        case .day(let menu):
            HStack(spacing: ThemeSpace.compact) {
                label
                Spacer()
                if let menu {
                    menu
                }
            }
            // At the covers' edge of rows that hang their times in the
            // margin, with room at the end over the rows' people.
            .padding(.trailing, ThemeSpace.edge)
            .padding(.top, ThemeSpace.group)
            .padding(.bottom, ThemeSpace.inline)
        }
    }

    @ViewBuilder
    private var label: some View {
        if let title {
            MacEyebrow(verbatim: title)
                .lineLimit(1)
        }
    }

    private func clearButton(_ clear: Clear) -> some View {
        Button(action: clear.action) {
            MacEyebrow("Clear")
        }
        .buttonStyle(.accentPlain)
        .help(clear.label)
        .accessibilityLabel(clear.label)
    }

}
