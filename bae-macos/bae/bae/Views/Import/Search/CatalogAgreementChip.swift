import BaeKit
import SwiftUI

/// A catalog number from the folder that agrees with an offered release.
/// Counted, it ranks that release up and badges its row; struck out, it counts
/// for nothing. Pressing it toggles that and re-ranks the same results.
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
            .background(
                background,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
            .overlay(
                RoundedRectangle(cornerRadius: ThemeRadius.chip)
                    .strokeBorder(border, lineWidth: 1)
            )
            .contentShape(RoundedRectangle(cornerRadius: ThemeRadius.chip))
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
            ? (isHovered ? Theme.accentStrong : Theme.accentSoft)
            : (isHovered ? Theme.hover : Color.clear)
    }

    private var border: Color {
        counted
            ? Color.clear
            : (isHovered ? Theme.hairlineStrong : Theme.hairline)
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
