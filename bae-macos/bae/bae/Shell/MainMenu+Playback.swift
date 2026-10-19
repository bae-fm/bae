import AppKit
import BaeKit

/// The Playback menu.
extension MainMenu {
    static func playbackMenu() -> NSMenu {
        menu(
            "Playback",
            [
                item(
                    "Play / Pause",
                    #selector(MainWindowController.playPause(_:)),
                    " ",
                    []
                ),
                item(
                    "Next Track",
                    #selector(MainWindowController.nextTrack(_:)),
                    rightArrowKey,
                    [.command, .option]
                ),
                item(
                    "Previous Track",
                    #selector(MainWindowController.previousTrack(_:)),
                    leftArrowKey,
                    [.command, .option]
                ),
                item(
                    "Mute",
                    #selector(MainWindowController.mute(_:)),
                    "m",
                    [.command, .option]
                ),
                .separator(),
                item(
                    "Cycle Repeat Mode",
                    #selector(MainWindowController.cycleRepeatMode(_:)),
                    "r"
                ),
                repeatItem(),
                .separator(),
                item(
                    "Pause between sides and discs",
                    #selector(MainWindowController.pauseBetweenSides(_:))
                ),
                item(
                    "Restore on launch",
                    #selector(AppDelegate.restorePlaybackOnLaunch(_:))
                ),
                .separator(),
                item(
                    "Shuffle Library",
                    #selector(MainWindowController.shuffleLibrary(_:))
                ),
            ]
        )
    }

    /// Playback ▸ Repeat: one checkmarked item per repeat mode.
    private static func repeatItem() -> NSMenuItem {
        submenu(
            "Repeat",
            RepeatModeChoice.allCases.map { choice in
                let item = NSMenuItem(
                    title: choice.title,
                    action: #selector(
                        MainWindowController.chooseRepeatMode(_:)
                    ),
                    keyEquivalent: ""
                )
                item.tag = choice.rawValue
                return item
            }
        )
    }

    /// `NSRightArrowFunctionKey` and `NSLeftArrowFunctionKey` as key
    /// equivalents.
    private static let rightArrowKey = "\u{F703}"
    private static let leftArrowKey = "\u{F702}"
}

/// The repeat modes Playback ▸ Repeat sets, by its items' tags.
enum RepeatModeChoice: Int, CaseIterable {
    case off
    case all
    case one

    var mode: BridgeRepeatMode {
        switch self {
        case .off: .off
        case .all: .context
        case .one: .track
        }
    }

    var title: String {
        switch self {
        case .off: String(localized: "Off")
        case .all: String(localized: "All")
        case .one: String(localized: "One")
        }
    }
}
