import BaeKit
import SwiftUI

/// The ⋯ button of a row, with the row's actions in its menu: a row offers
/// its actions here, not as a button of each. `boxed` draws it on a control's
/// fill, as a page's ⋯ beside Play; `label` names it to screen readers.
struct MacMoreMenu: View {
    var boxed = false
    var label = Text("More")
    let items: () -> [MacMenuEntry]

    init(
        boxed: Bool = false,
        label: Text = Text("More"),
        @MacMenuBuilder items: @escaping () -> [MacMenuEntry]
    ) {
        self.boxed = boxed
        self.label = label
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
                .background(
                    boxed ? Theme.hover : .clear,
                    in: RoundedRectangle(cornerRadius: ThemeRadius.control)
                )
                .contentShape(Rectangle())
        }
        .fixedSize()
        .accessibilityLabel(label)
    }
}
