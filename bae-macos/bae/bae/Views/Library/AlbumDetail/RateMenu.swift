import BaeKit
import SwiftUI

/// A Rate submenu, in a track's menu and an album's: the five ratings from
/// five stars down to one, each drawn as the stars, with the person's own
/// checked, then Clear rating. An album's leads with a disabled line of who
/// rated it.
struct RateMenu: MacMenuPart {
    /// The stars the person gave the track or album, if they did.
    let stars: BridgeStars?
    /// Who rated the album, named on the line at the top of the submenu;
    /// `nil` for a track and for an album nobody rated.
    var raters: [BridgeRater]?
    /// Rate with these stars, or clear the rating with none.
    let onRate: (BridgeStars?) -> Void

    var entries: [MacMenuEntry] {
        var items: [MacMenuEntry] = []
        if let raters {
            items.append(
                .item(MacMenuItem(verbatim: raters.text, disabled: true) {})
            )
            items.append(.divider)
        }
        items += BridgeStars.allCases.reversed()
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
