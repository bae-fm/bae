import AppKit

/// The View menu: the main window's sections and how the library shows.
extension MainMenu {
    static func viewMenu() -> NSMenu {
        menu(
            "View",
            [
                item(
                    "Library",
                    #selector(MainWindowController.showLibrarySection(_:)),
                    "1"
                ),
                item(
                    "Import",
                    #selector(MainWindowController.showImportSection(_:)),
                    "2"
                ),
                item(
                    "Storage Manager",
                    #selector(AppDelegate.showStorageManager(_:)),
                    "0"
                ),
                .separator(),
            ] + browserModeItems() + [
                .separator(),
                item(
                    "Full-Width Library",
                    #selector(MainWindowController.fullWidthLibrary(_:))
                ),
                .separator(),
                item(
                    "Search",
                    #selector(MainWindowController.focusSearch(_:)),
                    "/",
                    []
                ),
                .separator(),
                item(
                    "Go to Now Playing",
                    #selector(MainWindowController.goToNowPlaying(_:)),
                    "l"
                ),
                item(
                    "Toggle Queue",
                    #selector(MainWindowController.toggleQueue(_:)),
                    "s",
                    [.command, .shift]
                ),
                .separator(),
                item(
                    "Enter Full Screen",
                    #selector(NSWindow.toggleFullScreen(_:)),
                    "f",
                    [.command, .control]
                ),
            ]
        )
    }

    /// One checkmarked item per library browser mode.
    private static func browserModeItems() -> [NSMenuItem] {
        LibraryBrowserMode.allCases.enumerated()
            .map { index, mode in
                let item = NSMenuItem(
                    title: mode.displayName,
                    action: #selector(
                        MainWindowController.chooseLibraryBrowserMode(_:)
                    ),
                    keyEquivalent: ""
                )
                item.tag = index
                return item
            }
    }
}
