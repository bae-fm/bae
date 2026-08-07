import BaeKit
import SwiftUI

/// Shared sidebar layout: a surface-backed header above a divider, then the
/// scrollable content below. Unlike a plain toolbar, this header holds
/// several differently-padded sections (tab bar, filter row, scan progress)
/// stacked vertically, so each owns its own padding rather than the shell
/// applying one uniform inset.
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
                    .font(.headline)
                Spacer()
                Image(systemName: "plus")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
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
