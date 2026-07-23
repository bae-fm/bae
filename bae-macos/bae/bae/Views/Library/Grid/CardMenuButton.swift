import AppKit
import SwiftUI

/// The ellipsis button that overlays a hovered album card, popping the
/// `AlbumCardMenu` as a native `NSMenu` at the cursor.
struct CardMenuButton: View {
    let menu: AlbumCardMenu
    @Binding
    var showMenu: Bool
    @State
    private var isHovered = false

    var body: some View {
        Button(action: presentMenu) {
            Image(systemName: "ellipsis")
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(.white)
                .frame(width: 30, height: 30)
                .background(
                    isHovered ? Color.accentColor : Color.black.opacity(0.4)
                )
                .clipShape(Circle())
        }
        .buttonStyle(.plain)
        .onHover { isHovered = $0 }
    }

    private func presentMenu() {
        showMenu = true
        let nsMenu = NSMenu()
        nsMenu.addItem(MenuItem(title: menu.playLabel, handler: menu.onPlay))
        nsMenu.addItem(NSMenuItem.separator())
        nsMenu.addItem(
            MenuItem(title: menu.addToQueueLabel, handler: menu.onAddToQueue)
        )
        nsMenu.addItem(
            MenuItem(title: menu.addNextLabel, handler: menu.onAddNext)
        )
        nsMenu.addItem(MenuItem(title: menu.pinLabel, handler: menu.onPin))

        nsMenu.popUp(
            positioning: nil,
            at: NSEvent.mouseLocation,
            in: nil
        )
        showMenu = false
    }
}

private class MenuItem: NSMenuItem {
    private let handler: () -> Void

    init(title: String, handler: @escaping () -> Void) {
        self.handler = handler
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
    }

    @available(*, unavailable)
    required init(coder _: NSCoder) {
        fatalError()
    }

    @objc
    private func fire() {
        handler()
    }
}
