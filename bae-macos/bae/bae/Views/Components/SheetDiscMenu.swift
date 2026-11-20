import BaeKit
import SwiftUI

/// The Disc pop-up of a CUE sheet's caption: which of the release's discs
/// the sheet's tracks are on, and in the import pane that the sheet is
/// ignored.
struct SheetDiscMenu: View {
    /// The discs the pop-up offers, counted from one.
    let discs: [UInt32]
    let assignment: BridgeSheetDisc
    /// Whether the pop-up offers *Ignored* after the discs.
    let offersIgnored: Bool
    let onAssign: (BridgeSheetDisc) -> Void

    @State
    private var hovering = false

    var body: some View {
        MacButton(
            kind: .plain,
            menu: {
                for number in discs {
                    MacMenuItem(
                        verbatim: localizedCoreString(
                            "ui.import.sheet.disc",
                            Int(number)
                        ),
                        checked: assignment == .disc(number: number)
                    ) { onAssign(.disc(number: number)) }
                }
                if offersIgnored {
                    MacMenuItem.divider
                    MacMenuItem(
                        verbatim: localizedCoreString(
                            "ui.import.sheet.ignored"
                        ),
                        checked: assignment == .ignored
                    ) { onAssign(.ignored) }
                }
            }
        ) {
            HStack(spacing: ThemeSpace.inline) {
                Text(assignmentText)
                    .themeText(.strong)
                Image(systemName: "chevron.down")
                    .themeIcon(.small)
                    .foregroundStyle(.secondary)
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
        .help(localizedCoreString("ui.import.sheet.disc_help"))
        .accessibilityLabel(String(localized: "Disc"))
        .accessibilityValue(Text(verbatim: assignmentText))
    }

    private var assignmentText: String {
        switch assignment {
        case .disc(let number):
            localizedCoreString("ui.import.sheet.disc", Int(number))
        case .ignored:
            localizedCoreString("ui.import.sheet.ignored")
        }
    }
}
