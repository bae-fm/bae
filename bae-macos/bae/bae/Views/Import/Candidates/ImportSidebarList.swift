import BaeKit
import SwiftUI

/// The sidebar's layout: a header on the surface, a divider, then the content;
/// the header's sections pad themselves.
struct ImportSidebarList<Header: View, Content: View>: View {
    @ViewBuilder
    let header: () -> Header
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            header()
                .background(Theme.surface)
            Divider()
            content()
        }
    }
}

#if DEBUG
    // MARK: - Previews

#endif
