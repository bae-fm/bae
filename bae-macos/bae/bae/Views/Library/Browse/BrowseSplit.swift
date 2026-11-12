import BaeKit
import SwiftUI

/// The artist and composer modes' two panes: the master list at a fixed
/// width with a hairline on its edge, and the detail pane filling the rest.
struct BrowseSplit<ListPane: View, Detail: View>: View {
    private static var listWidth: CGFloat { 320 }

    @ViewBuilder
    let list: ListPane
    @ViewBuilder
    let detail: Detail

    var body: some View {
        HStack(spacing: 0) {
            list
                .frame(width: Self.listWidth)
                .overlay(alignment: .trailing) {
                    Rectangle()
                        .fill(Theme.hairline)
                        .frame(width: 1)
                }
            detail
                .frame(maxWidth: .infinity)
        }
    }
}
