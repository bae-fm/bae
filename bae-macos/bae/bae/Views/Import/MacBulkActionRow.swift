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
        if let importDestination {
            // The row's button imports and the destination menu beside it
            // picks where, as two controls: a menu inside a button would
            // make the whole row read as a pop-up.
            HStack(spacing: ImportBulkActionRowMetrics.spacing) {
                Button(action: action) {
                    nameAndSymbol
                }
                .buttonStyle(.plain)
                .accentFocus()
                .modifier(
                    NamedByLabelAndCount(
                        offer: offer,
                        name:
                            "\(offer.action.label) \(importDestination.wrappedValue.title)"
                    )
                )
                MacImportDestination(destination: importDestination)
                count
            }
            .modifier(BulkRowSurface(isPressed: false))
        }
        else {
            Button(action: action) {
                HStack(spacing: ImportBulkActionRowMetrics.spacing) {
                    nameAndSymbol
                    count
                }
            }
            .buttonStyle(MacBulkActionRowButtonStyle().accentFocus())
            .modifier(
                NamedByLabelAndCount(offer: offer, name: offer.action.label)
            )
        }
    }

    /// The action's symbol and its name, the part that takes the row's width.
    private var nameAndSymbol: some View {
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
        }
    }

    private var count: some View {
        StatusChip(
            Text(verbatim: offer.applicable.formatted())
                .monospacedDigit(),
            tone: isConstructive ? .accent : .neutral
        )
    }
}

/// Names a row's button for VoiceOver by its action name and its count in
/// one phrase, as the board's button reads, where SwiftUI would join the two
/// with a comma.
private struct NamedByLabelAndCount: ViewModifier {
    let offer: ImportCandidateActionOffer
    let name: String

    func body(content: Content) -> some View {
        content
            .accessibilityLabel(
                String(
                    localized:
                        "Bulk action \(name) \(offer.applicable.formatted())"
                )
            )
    }
}

/// A full-width row with a hover and press fill; the group clips its corners.
struct MacBulkActionRowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Row(configuration: configuration)
    }

    private struct Row: View {
        let configuration: Configuration

        var body: some View {
            configuration.label
                .modifier(BulkRowSurface(isPressed: configuration.isPressed))
        }
    }
}

/// A bulk row's padding and its hover and press fill, with the row dimmed
/// while its action is off.
private struct BulkRowSurface: ViewModifier {
    let isPressed: Bool
    @Environment(\.isEnabled)
    private var isEnabled
    @State
    private var isHovered = false

    func body(content: Content) -> some View {
        content
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
        if isPressed {
            return Theme.pressed
        }
        return isHovered ? Theme.hover : .clear
    }
}
