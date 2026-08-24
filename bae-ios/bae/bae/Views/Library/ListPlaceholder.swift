import SwiftUI

/// The full-height area a library list shows in place of rows — its empty
/// message, its load failure, or the spinner before the list exists.
///
/// The content sits inside a scroll view that fills the viewport and always
/// bounces, because `.refreshable` only arms the scroll view underneath it: a
/// bare centered view gives the pull gesture nothing to grab, which is why an
/// empty Albums/Composers/Artists tab used to refuse pull-to-refresh while a
/// populated one accepted it.
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
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(32)
    }
}
#endif
