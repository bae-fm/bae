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

#if DEBUG
    // MARK: - Previews

    #Preview("Sidebar list") {
        ImportSidebarList {
            Text("Header")
                .font(.headline)
            Spacer()
            Image(systemName: "plus")
        } content: {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(0..<5) { index in
                    Text(verbatim: "Row \(index + 1)")
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                }
                Spacer()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(width: 280, height: 360)
        .windowBackground()
    }
#endif
