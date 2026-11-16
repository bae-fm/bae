import BaeKit
import SwiftUI

/// A track menu's Rate submenu: the five ratings from five stars down to one,
/// each drawn as the stars, with the person's own checked, then Clear rating.
struct TrackRateMenu: MacMenuPart {
    /// The stars the person gave the track, if they did.
    let stars: BridgeStars?
    /// Rate the track with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void

    var entries: [MacMenuEntry] {
        var items: [MacMenuEntry] = BridgeStars.allCases.reversed()
            .map { choice in
                .item(
                    MacMenuItem(
                        "\(choice.number) of 5 stars",
                        image: Stars(value: choice.number, size: 11),
                        checked: choice == stars
                    ) { onRate(choice) }
                )
            }
        items.append(.divider)
        items.append(
            .item(
                MacMenuItem("Clear rating", disabled: stars == nil) {
                    onRate(nil)
                }
            )
        )
        return [.submenu(String(localized: "Rate"), items, nil)]
    }
}
