import BaeKit
import SwiftUI

/// One bulk action: the action's symbol, its name, and how many of
/// the selected folders it applies to. The Import row also holds where the
/// import goes, before its count.
struct MacBulkActionRow: View {
    let offer: ImportCandidateActionOffer
    let importDestination: Binding<BridgeImportDestination>?
    let action: () -> Void

    private var isConstructive: Bool { offer.isConstructive }

    var body: some View {
        Button(action: action) {
            HStack(spacing: ImportBulkActionRowMetrics.spacing) {
                Image(systemName: offer.action.symbol)
                    .themeIcon(ImportBulkActionRowMetrics.icon)
                    .foregroundStyle(
                        isConstructive ? Theme.accent : Theme.macTextSecondary
                    )
                    .frame(width: ImportBulkActionRowMetrics.iconWidth)
                Text(verbatim: offer.action.label)
                    .themeText(isConstructive ? .strong : .body)
                    .foregroundStyle(
                        isConstructive
                            ? Theme.textPrimary : Theme.macTextSecondary
                    )
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let importDestination {
                    MacImportDestination(destination: importDestination)
                }
                StatusChip(
                    Text(verbatim: offer.applicable.formatted())
                        .monospacedDigit(),
                    tone: isConstructive ? .accent : .neutral
                )
            }
        }
        .buttonStyle(MacBulkActionRowButtonStyle().accentFocus())
        .modifier(
            NamedByLabelAndCount(
                offer: offer,
                named: importDestination == nil
            )
        )
    }
}

/// Names a row for VoiceOver by its action and its count in one phrase, as
/// the board's button reads, where SwiftUI would join the two with a comma.
/// The Import row holds a menu of its own, so it keeps its parts apart.
private struct NamedByLabelAndCount: ViewModifier {
    let offer: ImportCandidateActionOffer
    let named: Bool

    func body(content: Content) -> some View {
        if named {
            content
                .accessibilityLabel(
                    String(
                        localized:
                            "Bulk action \(offer.action.label) \(offer.applicable.formatted())"
                    )
                )
        }
        else {
            content
        }
    }
}

/// A full-width row with a hover and press fill; the group clips its corners.
struct MacBulkActionRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Row(configuration: configuration)
    }

    private struct Row: View {
        let configuration: Configuration
        @Environment(\.isEnabled)
        private var isEnabled
        @State
        private var isHovered = false

        var body: some View {
            configuration.label
                .padding(.vertical, ImportBulkActionRowMetrics.vertical)
                .padding(.horizontal, ImportBulkActionRowMetrics.horizontal)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(fill)
                .contentShape(Rectangle())
                .opacity(isEnabled ? 1 : 0.4)
                .onHover { isHovered = $0 }
        }

        private var fill: Color {
            guard isEnabled else { return .clear }
            if configuration.isPressed {
                return Theme.pressed
            }
            return isHovered ? Theme.hover : .clear
        }
    }
}
