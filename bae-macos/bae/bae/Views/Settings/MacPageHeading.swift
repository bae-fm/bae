import BaeKit
import SwiftUI

/// The heading of a settings pane: bold, with the room under it the first box
/// starts after. The pane sets it in from its top and sides
/// (`placedInPane()`). On a page inside a pane it starts with a
/// `MacBackButton`, in the place and type a pane's own title has.
struct MacPageHeading: View {
    /// The button back to the page before.
    struct Back {
        /// The title of the page before.
        let title: String
        let action: () -> Void
    }

    let title: Text
    var back: Back?

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            if let back {
                MacBackButton(backTitle: back.title, onBack: back.action)
            }
            title
        }
        .themeText(.heading)
        .lineLimit(1)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, ThemeSpace.edge)
    }

    /// The heading at the pane's top left.
    func placedInPane() -> some View {
        padding(.horizontal, SettingsPaneMetrics.sideInset)
            .padding(.top, SettingsPaneMetrics.topInset)
    }
}
