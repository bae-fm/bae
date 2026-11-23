import BaeKit
import SwiftUI

/// The place pop-up of a CUE sheet's caption: which of the release's sides
/// and discs the sheet's tracks are on, each named by its heading, and,
/// where core offers it, that the sheet is ignored.
struct SheetPlaceMenu: View {
    /// The sides and discs the pop-up offers, in order.
    let choices: [BridgeSheetSideChoice]
    /// The side or disc the sheet is on, with its heading; `nil` where the
    /// sheet is ignored.
    let assigned: BridgeSheetSideChoice?
    /// Whether the pop-up offers *Ignored* after the sides and discs.
    let offersIgnored: Bool
    let onAssign: (BridgeSheetPlace) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                for choice in choices {
                    MacMenuItem(
                        verbatim: choice.heading.headingText,
                        checked: assigned?.side == choice.side
                    ) { onAssign(.on(side: choice.side)) }
                }
                if offersIgnored {
                    MacMenuItem.divider
                    MacMenuItem(
                        verbatim: localizedCoreString(
                            "ui.import.sheet.ignored"
                        ),
                        checked: assigned == nil
                    ) { onAssign(.ignored) }
                }
            }
        ) {
            HStack(spacing: ThemeSpace.inline) {
                Text(assignmentText)
                    .themeText(.strong)
                Image(systemName: "chevron.down")
                    .themeIcon(.small)
                    .foregroundStyle(Theme.textSecondary)
            }
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                hovering ? Theme.pressed : Theme.hover,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
        }
        .fixedSize()
        .onHover { hovering = $0 }
        .help(localizedCoreString("ui.import.sheet.place_help"))
        .accessibilityLabel(String(localized: "Side or disc"))
        .accessibilityValue(Text(verbatim: assignmentText))
    }

    /// The heading of the side or disc the sheet is on, or *Ignored*.
    private var assignmentText: String {
        guard let assigned else {
            return localizedCoreString("ui.import.sheet.ignored")
        }
        return assigned.heading.headingText
    }
}
