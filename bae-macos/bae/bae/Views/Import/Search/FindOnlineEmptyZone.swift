import BaeKit
import SwiftUI

/// A section with nothing to list: in the middle, one line saying what
/// happened with the one thing to do about it, stacked.
struct FindOnlineEmptyZone<Content: View>: View {
    @ViewBuilder
    let content: Content

    var body: some View {
        VStack(spacing: ThemeSpace.related) {
            content
        }
        .themeText(.body)
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.section)
        .frame(maxWidth: .infinity)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Nothing found") {
        FindOnlineEmptyZone {
            Text("No results")
                .foregroundStyle(.secondary)
            SearchManuallyButton(action: {})
        }
        .frame(width: 620)
        .windowBackground()
    }

    #Preview("Not looked up") {
        FindOnlineEmptyZone {
            SearchManuallyButton(action: {})
        }
        .frame(width: 620)
        .windowBackground()
    }
#endif
