import BaeKit
import SwiftUI

/// A text-only detail row (a composer credit or a recording): title over an
/// optional secondary line, no image slot and no hover — nothing to open.
struct CreditRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            Text(title)
                .themeText(.rowTitle)
                .lineLimit(1)
            StableOptionalText(
                text: subtitle,
                font: ThemeText.detail.font,
                foreground: .secondary,
                lineHeight: 14,
                lineLimit: 1
            )
        }
        .padding(.vertical, ThemeSpace.inline)
    }
}

#if DEBUG
    #Preview("Credit Row") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            CreditRow(title: "Album Title", subtitle: "Arranger")
            CreditRow(title: "Track Title", subtitle: nil)
        }
        .padding()
        .frame(width: 420, alignment: .leading)
    }
#endif
