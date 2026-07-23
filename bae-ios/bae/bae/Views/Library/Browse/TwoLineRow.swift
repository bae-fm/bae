import SwiftUI

/// A title over an optional secondary line — the shared row body for the
/// composer, artist, and work browse lists.
struct TwoLineRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.body)
                .lineLimit(1)
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

#if DEBUG
#Preview {
    List {
        TwoLineRow(title: "Title", subtitle: "Subtitle")
        TwoLineRow(title: "Title only", subtitle: nil)
    }
}
#endif
