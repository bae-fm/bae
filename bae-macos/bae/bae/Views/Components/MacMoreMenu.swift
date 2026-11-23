import BaeKit
import SwiftUI

/// The ⋯ button of a row, with the row's actions in its menu: a row offers
/// its actions here, not as a button of each. `overlay` draws the button
/// round on a dark fill, for a place on a cover; `boxed` draws it on the
/// push button's fill, beside a page's Play. `label` names it to screen
/// readers.
struct MacMoreMenu: View {
    /// How the button looks.
    enum Look {
        case plain
        case boxed
        case overlay
    }

    let look: Look
    let label: Text
    /// True while the menu is open.
    let isOpen: Binding<Bool>?
    let items: () -> [MacMenuEntry]

    @State
    private var isHovered = false

    init(
        look: Look = .plain,
        label: Text = Text("More"),
        isOpen: Binding<Bool>? = nil,
        @MacMenuBuilder items: @escaping () -> [MacMenuEntry]
    ) {
        self.look = look
        self.label = label
        self.isOpen = isOpen
        self.items = items
    }

    var body: some View {
        Group {
            switch look {
            case .plain:
                MacIconButton.menu(
                    symbol: "ellipsis",
                    label: "More",
                    iconSize: .medium,
                    isOpen: isOpen,
                    items: items
                )
            case .boxed:
                MacButton(kind: .plain, isOpen: isOpen, menu: items) {
                    boxedDots
                }
            case .overlay:
                MacButton(kind: .plain, isOpen: isOpen, menu: items) {
                    overlayDots
                }
            }
        }
        .fixedSize()
        .onHover { isHovered = $0 }
        .accessibilityLabel(label)
    }

    /// The dots on the push button's boxed face, as the design draws a boxed
    /// icon button.
    private var boxedDots: some View {
        BoxedIconFace(size: .control, isPressed: false) {
            Image(systemName: "ellipsis")
                .themeIcon(.medium)
        }
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
