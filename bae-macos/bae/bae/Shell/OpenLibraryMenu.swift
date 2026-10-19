import AppKit
import BaeKit

/// File ▸ Open Library: one item per library on this Mac, the active one
/// checked and the first nine on ⇧⌘1 to ⇧⌘9 so libraries can be switched
/// without opening the menu; then the previous and next library.
///
/// The items follow the library list while the menu is closed, so their key
/// equivalents work without it opening. While it is open it keeps the items
/// it opened with, and takes the list's newest once it closes: the menu on
/// screen is laid out from its items.
@MainActor
final class OpenLibraryMenu: NSObject, NSMenuDelegate {
    let menu = NSMenu(title: String(localized: "Open Library"))
    private var libraries: [BridgeLibrary] = []
    private var isOpen = false

    override init() {
        super.init()
        menu.delegate = self
        menu.items = items()
    }

    /// Show `libraries`, now or once the menu closes.
    func show(_ libraries: [BridgeLibrary]) {
        self.libraries = libraries
        if !isOpen {
            menu.items = items()
        }
    }

    func menuWillOpen(_: NSMenu) {
        isOpen = true
    }

    func menuDidClose(_: NSMenu) {
        isOpen = false
        menu.items = items()
    }

    private func items() -> [NSMenuItem] {
        var items: [NSMenuItem] = []
        if libraries.isEmpty {
            // No action, so AppKit leaves it disabled.
            items.append(MainMenu.item("No Libraries", nil))
        }
        for (index, library) in libraries.enumerated() {
            let item = NSMenuItem(
                title: library.name,
                action: #selector(AppDelegate.openListedLibrary(_:)),
                keyEquivalent: index < 9 ? String(index + 1) : ""
            )
            item.keyEquivalentModifierMask = [.command, .shift]
            item.representedObject = library.id
            item.state = library.isActive ? .on : .off
            items.append(item)
        }
        items.append(.separator())
        items.append(
            MainMenu.item(
                "Previous Library",
                #selector(MainWindowController.previousLibrary(_:)),
                "[",
                [.command, .shift]
            )
        )
        items.append(
            MainMenu.item(
                "Next Library",
                #selector(MainWindowController.nextLibrary(_:)),
                "]",
                [.command, .shift]
            )
        )
        return items
    }
}
