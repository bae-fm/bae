import BaeKit

/// A track menu's Rate submenu: the five star counts from five down to one
/// with the person's own checked, then Clear rating.
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
                        verbatim: String(repeating: "★", count: choice.number),
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
