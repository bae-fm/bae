import BaeKit
import SwiftUI

/// The ⋯ button of a row, with the row's actions in its menu: a row offers
/// its actions here, not as a button of each.
struct MacMoreMenu<Items: View>: View {
    @ViewBuilder
    let items: Items

    var body: some View {
        Menu {
            items
        } label: {
            Image(systemName: "ellipsis")
                // The design draws the dots at 15, a size above the icon roles.
                .font(.system(size: 15, weight: ThemeIcon.medium.weight))
                .frame(
                    width: ThemeSize.hitTarget,
                    height: ThemeSize.hitTarget
                )
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .tint(.secondary)
        .fixedSize()
        .accessibilityLabel(Text("More"))
    }
}
