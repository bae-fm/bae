import BaeKit
import SwiftUI

/// The artist and composer modes' two panes: the master list at a fixed
/// width, inset and on the background colour, with a hairline on its edge,
/// and the detail pane filling the rest, inset and on the surface colour.
struct MacBrowseSplit<ListPane: View, Detail: View>: View {
    private static var listWidth: CGFloat { 320 }

    @ViewBuilder
    let list: ListPane
    @ViewBuilder
    let detail: Detail

    var body: some View {
        HStack(spacing: 0) {
            list
                .safeAreaPadding(.horizontal, ThemeSpace.related)
                .safeAreaPadding(.top, ThemeSpace.inline)
                .safeAreaPadding(.bottom, ThemeSpace.section)
                .frame(width: Self.listWidth)
                .frame(maxHeight: .infinity, alignment: .top)
                .background(Theme.background)
                .overlay(alignment: .trailing) {
                    MacHairline(axis: .vertical)
                }
            detail
                .safeAreaPadding(ThemeSpace.section)
                .frame(
                    maxWidth: .infinity,
                    maxHeight: .infinity,
                    alignment: .topLeading
                )
                .background(Theme.surface)
        }
    }
}
