import BaeKit
import SwiftUI

/// One catalog number the folder states about a release identification is
/// offering. Counted, it is an agreement: it ranks that release up the list
/// and badges its row. Struck out, it counts for nothing — the number on the
/// sleeve that turned out to be a phone number, a serial, the year again.
///
/// Pressing it turns it over. Nothing is asked again: the same answers are
/// ranked by what the folder is now taken to state about them, so the chip
/// carries the same accent tint the Catalog badge on the rows does.
struct CatalogAgreementChip: View {
    let agreement: BridgeCatalogAgreement
    let onToggle: () -> Void

    @State
    private var isHovered = false

    private var counted: Bool { !agreement.discounted }

    var body: some View {
        Button(action: onToggle) {
            HStack(spacing: 5) {
                Image(
                    systemName: counted
                        ? "checkmark.circle.fill" : "circle.dashed"
                )
                .font(.system(size: 10, weight: .semibold))
                Text(agreement.value)
                    .font(.system(size: 10.5, design: .monospaced))
                    .strikethrough(!counted)
                    .lineLimit(1)
            }
            .foregroundStyle(foreground)
            .padding(.leading, 6)
            .padding(.trailing, 9)
            .padding(.vertical, 4)
            .background(background, in: RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(border, lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
        .help(
            counted
                ? String(localized: "Stop counting this catalog number")
                : String(localized: "Count this catalog number again")
        )
    }

    private var foreground: AnyShapeStyle {
        counted ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.tertiary)
    }

    private var background: Color {
        counted
            ? Color.accentColor.opacity(isHovered ? 0.24 : 0.15)
            : Color.primary.opacity(isHovered ? 0.04 : 0)
    }

    private var border: Color {
        counted
            ? Color.clear
            : Color.primary.opacity(isHovered ? 0.18 : 0.09)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Catalog agreement chips") {
        FlowLayout(spacing: 6) {
            CatalogAgreementChip(
                agreement: BridgeCatalogAgreement(
                    value: "16033-2",
                    discounted: false
                ),
                onToggle: {}
            )
            CatalogAgreementChip(
                agreement: BridgeCatalogAgreement(
                    value: "7567-92413-2",
                    discounted: true
                ),
                onToggle: {}
            )
        }
        .padding()
        .frame(width: 400)
        .windowBackground()
    }
#endif
