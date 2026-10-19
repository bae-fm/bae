import AppKit

/// bae's main menu: the standard macOS menus with their standard actions,
/// which AppKit sends up the responder chain and enables from what the
/// responder that takes them says, plus bae's own commands. Built once at
/// launch; nothing replaces its items after.
///
/// bae's items have no target either: AppKit sends each to the first
/// responder that takes it. The app's commands are `AppDelegate`'s, at the
/// end of every chain; a library's are the primary window's
/// `MainWindowController`, in the chain only while that window is key or
/// main, so they are disabled while another window is. Select All alone has
/// a target, `SelectAllTarget`, which sends it on up the chain except to a
/// focused list that selects rows its table has not loaded.
@MainActor
enum MainMenu {
    /// The menu, with `openLibrary` as File ▸ Open Library and Edit ▸
    /// Select All sent to `selectAll`.
    static func make(
        openLibrary: NSMenu,
        selectAll: SelectAllTarget
    ) -> NSMenu {
        let windowMenu = Self.windowMenu()
        let helpMenu = Self.helpMenu()
        let servicesMenu = NSMenu(title: String(localized: "Services"))
        let mainMenu = NSMenu(title: "")
        for menu in [
            appMenu(servicesMenu: servicesMenu),
            fileMenu(openLibrary: openLibrary),
            editMenu(selectAll: selectAll),
            viewMenu(),
            playbackMenu(),
            windowMenu,
            helpMenu,
        ] {
            let item = NSMenuItem(
                title: menu.title,
                action: nil,
                keyEquivalent: ""
            )
            item.submenu = menu
            mainMenu.addItem(item)
        }
        NSApplication.shared.servicesMenu = servicesMenu
        NSApplication.shared.windowsMenu = windowMenu
        NSApplication.shared.helpMenu = helpMenu
        return mainMenu
    }

    // MARK: - Menus

    private static func appMenu(servicesMenu: NSMenu) -> NSMenu {
        let services = NSMenuItem(
            title: servicesMenu.title,
            action: nil,
            keyEquivalent: ""
        )
        services.submenu = servicesMenu
        return menu(
            "bae",
            [
                item(
                    "About bae",
                    #selector(
                        NSApplication.orderFrontStandardAboutPanel(_:)
                    )
                ),
                item(
                    "Check for Updates...",
                    #selector(AppDelegate.checkForUpdates(_:))
                ),
                .separator(),
                item(
                    "Settings...",
                    #selector(AppDelegate.showSettingsWindow(_:)),
                    ","
                ),
                .separator(),
                services,
                .separator(),
                item("Hide bae", #selector(NSApplication.hide(_:)), "h"),
                item(
                    "Hide Others",
                    #selector(NSApplication.hideOtherApplications(_:)),
                    "h",
                    [.command, .option]
                ),
                item(
                    "Show All",
                    #selector(NSApplication.unhideAllApplications(_:))
                ),
                .separator(),
                item("Quit bae", #selector(NSApplication.terminate(_:)), "q"),
            ]
        )
    }

    private static func fileMenu(openLibrary libraries: NSMenu) -> NSMenu {
        let openLibrary = NSMenuItem(
            title: libraries.title,
            action: nil,
            keyEquivalent: ""
        )
        openLibrary.submenu = libraries
        return menu(
            "File",
            [
                item(
                    "New Library...",
                    #selector(AppDelegate.newLibrary(_:)),
                    "n",
                    [.command, .option]
                ),
                item(
                    "Join a Library...",
                    #selector(AppDelegate.joinLibrary(_:))
                ),
                item(
                    "Restore from Code...",
                    #selector(AppDelegate.restoreFromCode(_:))
                ),
                openLibrary,
                .separator(),
            ] + libraryItems()
        )
    }

    /// The File menu's commands for the open library, then Close.
    private static func libraryItems() -> [NSMenuItem] {
        [
            item(
                "Import Folder...",
                #selector(MainWindowController.importFolder(_:)),
                "i"
            ),
            .separator(),
            item(
                "Rename Library...",
                #selector(MainWindowController.renameLibrary(_:))
            ),
            item(
                "Sync Now",
                #selector(MainWindowController.syncNow(_:))
            ),
            item(
                "Reveal Library in Finder",
                #selector(MainWindowController.revealLibraryInFinder(_:))
            ),
            item(
                "Copy Library ID",
                #selector(MainWindowController.copyLibraryId(_:))
            ),
            .separator(),
            item(
                "Close Library",
                #selector(MainWindowController.closeLibrary(_:)),
                "w",
                [.command, .shift]
            ),
            item("Close", #selector(NSWindow.performClose(_:)), "w"),
        ]
    }

    private static func windowMenu() -> NSMenu {
        menu(
            "Window",
            [
                item(
                    "Minimize",
                    #selector(NSWindow.performMiniaturize(_:)),
                    "m"
                ),
                item("Zoom", #selector(NSWindow.performZoom(_:))),
                .separator(),
                item(
                    "Bring All to Front",
                    #selector(NSApplication.arrangeInFront(_:))
                ),
            ]
        )
    }

    private static func helpMenu() -> NSMenu {
        menu(
            "Help",
            [item("bae Help", #selector(NSApplication.showHelp(_:)), "?")]
        )
    }

    // MARK: - Building

    static func menu(
        _ title: String.LocalizationValue,
        _ items: [NSMenuItem]
    ) -> NSMenu {
        let menu = NSMenu(title: String(localized: title))
        menu.items = items
        return menu
    }

    static func submenu(
        _ title: String.LocalizationValue,
        _ items: [NSMenuItem]
    ) -> NSMenuItem {
        let submenu = menu(title, items)
        let item = NSMenuItem(
            title: submenu.title,
            action: nil,
            keyEquivalent: ""
        )
        item.submenu = submenu
        return item
    }

    static func item(
        _ title: String.LocalizationValue,
        _ action: Selector?,
        _ keyEquivalent: String = "",
        _ modifiers: NSEvent.ModifierFlags = .command,
        tag: NSTextFinder.Action? = nil
    ) -> NSMenuItem {
        let item = NSMenuItem(
            title: String(localized: title),
            action: action,
            keyEquivalent: keyEquivalent
        )
        item.keyEquivalentModifierMask = modifiers
        if let tag {
            item.tag = tag.rawValue
        }
        return item
    }
}
