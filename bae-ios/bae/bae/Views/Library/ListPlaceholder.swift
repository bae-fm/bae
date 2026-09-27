import BaeKit
import SwiftUI

/// A library list's full-height empty, failed or loading state, in an
/// always-bouncing scroll view so `.refreshable` has something to pull.
struct ListPlaceholder<Content: View>: View {
    @ViewBuilder
    var content: Content

    var body: some View {
        GeometryReader { proxy in
            ScrollView {
                content
                    .frame(maxWidth: .infinity, minHeight: proxy.size.height)
            }
            .scrollBounceBehavior(.always)
        }
    }
}

#if DEBUG
#Preview {
    ListPlaceholder {
        Text(verbatim: "No albums yet.")
            .themeText(.body)
            .foregroundStyle(.secondary)
            .padding(ThemeSpace.page)
    }
}
#endif
