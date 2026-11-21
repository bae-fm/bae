import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacMenuItem`, one shot for each row of its preview.
    static let macMenuItemScenes: [ShotScene] = [
        component("mac.component.menu-item.plain", height: 100) {
            MacMenuInPlace {
                MacMenuItem("Only me") {}
            }
        },
        component("mac.component.menu-item.checked", height: 100) {
            MacMenuInPlace {
                MacMenuItem("Only me", checked: true) {}
            }
        },
        component("mac.component.menu-item.highlighted", height: 100) {
            MacMenuInPlace(
                {
                    MacMenuItem("Only me") {}
                },
                highlighted: 0
            )
        },
        component("mac.component.menu-item.off", height: 100) {
            MacMenuInPlace {
                MacMenuItem("Only me", disabled: true) {}
            }
        },
        component("mac.component.menu-item.heading", height: 120) {
            MacMenuInPlace {
                MacMenuSection("Folders") {
                    MacMenuItem("Music") {}
                }
            }
        },
        component("mac.component.menu-item.divider", height: 140) {
            MacMenuInPlace {
                MacMenuItem("Refresh") {}
                MacMenuItem.divider
                MacMenuItem("Remove folder") {}
            }
        },
        component("mac.component.menu-item.submenu", height: 100) {
            MacMenuInPlace {
                MacMenuSubmenu(verbatim: "Music") {}
            }
        },
        component(
            "mac.component.menu-item.submenu-with-a-problem",
            height: 100
        ) {
            MacMenuInPlace {
                MacMenuSubmenu(
                    verbatim: "Downloads",
                    accessory: Self.problemAccessory
                ) {}
            }
        },
        component(
            "mac.component.menu-item.submenu-highlighted",
            height: 100
        ) {
            MacMenuInPlace(
                {
                    MacMenuSubmenu(
                        verbatim: "Downloads",
                        accessory: Self.problemAccessory
                    ) {}
                },
                highlighted: 0
            )
        },
        component("mac.component.menu-item.submenu-scanning", height: 100) {
            MacMenuInPlace {
                MacMenuSubmenu(
                    verbatim: "Music",
                    accessory: MacMenuAccessory(
                        symbol: "progress.indicator",
                        tone: .secondary
                    )
                ) {}
            }
        },
        component(
            "mac.component.menu-item.submenu-on-the-network",
            height: 100
        ) {
            MacMenuInPlace {
                MacMenuSubmenu(
                    verbatim: "Shared Music",
                    accessory: MacMenuAccessory(
                        symbol: "network",
                        tone: .secondary,
                        help:
                            "bae checks this folder for changes every 5 minutes."
                    )
                ) {}
            }
        },
    ]

    /// `MacMenuItem` plain, checked, highlighted, off, as a heading, a divider
    /// and a submenu in each state, and an open `MacMenu`, drawn in place.
    private static let problemAccessory = MacMenuAccessory(
        symbol: "exclamationmark.circle",
        tone: .danger,
        help:
            "bae reached the system’s limit on watched folders at Downloads/Albums/2024."
    )
}
