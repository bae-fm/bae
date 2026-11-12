import BaeKit
import SwiftUI

/// The ⋯ button of a row, with the row's actions in its menu: a row offers
/// its actions here, not as a button of each.
struct MacMoreMenu: View {
    let items: () -> [MacMenuEntry]

    init(@MacMenuBuilder items: @escaping () -> [MacMenuEntry]) {
        self.items = items
    }

    var body: some View {
        MacMenuButton(content: items) {
            Image(systemName: "ellipsis")
                // The design draws the dots at 15, a size above the icon roles.
                .font(.system(size: 15, weight: ThemeIcon.medium.weight))
                .foregroundStyle(.secondary)
                .frame(
                    width: ThemeSize.hitTarget,
                    height: ThemeSize.hitTarget
                )
                .contentShape(Rectangle())
        }
        .fixedSize()
        .accessibilityLabel(Text("More"))
    }
}
