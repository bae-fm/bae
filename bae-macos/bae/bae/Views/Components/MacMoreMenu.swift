import BaeKit
import SwiftUI

/// The ⋯ button of a row, with the row's actions in its menu: a row offers
/// its actions here, not as a button of each. `overlay` draws the button
/// round on a dark fill, for a place on a cover.
struct MacMoreMenu: View {
    let overlay: Bool
    /// True while the menu is open.
    let isOpen: Binding<Bool>?
    let items: () -> [MacMenuEntry]

    @State
    private var isHovered = false

    init(
        overlay: Bool = false,
        isOpen: Binding<Bool>? = nil,
        @MacMenuBuilder items: @escaping () -> [MacMenuEntry]
    ) {
        self.overlay = overlay
        self.isOpen = isOpen
        self.items = items
    }

    var body: some View {
        MacButton(kind: .plain, isOpen: isOpen, menu: items) {
            if overlay {
                overlayDots
            }
            else {
                dots
            }
        }
        .fixedSize()
        .onHover { isHovered = $0 }
        .accessibilityLabel(Text("More"))
    }

    private var dots: some View {
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

    /// The dots on a round fill that turns to the accent under the pointer.
    private var overlayDots: some View {
        Image(systemName: "ellipsis")
            .themeIcon(.medium)
            .foregroundStyle(Theme.onFill)
            .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
            .background(isHovered ? Theme.accent : Theme.scrim)
            .clipShape(Circle())
    }
}
