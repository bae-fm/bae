import BaeKit
import SwiftUI

/// The full-height area a library list shows in place of rows: its empty
/// message, its load failure, or a spinner. It sits in an always-bouncing
/// scroll view because `.refreshable` needs one underneath to pull on.
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
            .padding(32)
    }
}
#endif
