import BaeKit
import SwiftUI

/// Shared sidebar layout: a surface-backed header row above a divider, then the
/// scrollable content below.
struct ImportSidebarList<Header: View, Content: View>: View {
    @ViewBuilder
    let header: () -> Header
    @ViewBuilder
    let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                header()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(Theme.surface)
            Divider()
            content()
        }
    }
}
