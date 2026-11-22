import BaeKit

/// The Heart item of an album's menu and of a track's menu under the Heart
/// rating system: Add heart, or Remove heart on one the person hearted.
struct HeartMenuItem: MacMenuPart {
    /// Whether the person has a heart on the album or the track.
    let hearted: Bool
    /// Turn the person's heart on or off.
    let onSetHearted: (Bool) -> Void

    var entries: [MacMenuEntry] {
        [
            .item(
                hearted
                    ? MacMenuItem("Remove heart") { onSetHearted(false) }
                    : MacMenuItem("Add heart") { onSetHearted(true) }
            )
        ]
    }
}
