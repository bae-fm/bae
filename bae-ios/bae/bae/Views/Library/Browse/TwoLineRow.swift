import BaeKit
import SwiftUI

/// A browse row's title over an optional secondary line.
struct TwoLineRow: View {
    let title: String
    let subtitle: String?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            Text(title)
                .themeText(.rowTitle)
                .lineLimit(1)
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .themeText(.detail)
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
