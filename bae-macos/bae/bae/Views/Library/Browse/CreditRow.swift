import SwiftUI

/// A text-only detail row (a composer credit or a recording): title over an
/// optional secondary line, no image slot and no hover — nothing to open.
struct CreditRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.browseRowTitle)
                .lineLimit(1)
            StableOptionalText(
                text: subtitle,
                font: .browseRowCaption,
                foreground: .secondary,
                lineHeight: 14,
                lineLimit: 1
            )
        }
        .padding(.vertical, 4)
    }
}

#if DEBUG
    #Preview("Credit Row") {
        VStack(alignment: .leading, spacing: 8) {
            CreditRow(title: "Album Title", subtitle: "Arranger")
            CreditRow(title: "Track Title", subtitle: nil)
        }
        .padding()
        .frame(width: 420, alignment: .leading)
    }
#endif
