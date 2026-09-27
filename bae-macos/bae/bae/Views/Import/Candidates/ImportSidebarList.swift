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

    #Preview("Sidebar list") {
        ImportSidebarList {
            HStack {
                Text(verbatim: "Header")
                    .themeText(.heading)
                Spacer()
                Image(systemName: "plus")
            }
            .padding(.horizontal, ThemeSpace.group)
            .padding(.vertical, ThemeSpace.related)
        } content: {
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                ForEach(0..<5) { index in
                    Text(verbatim: "Row \(index + 1)")
                        .padding(.horizontal, ThemeSpace.group)
                        .padding(.vertical, ThemeSpace.inline)
                }
                Spacer()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(width: 280, height: 360)
        .windowBackground()
    }
#endif
