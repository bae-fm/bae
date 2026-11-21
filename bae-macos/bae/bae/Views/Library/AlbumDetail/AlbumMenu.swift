import BaeKit

/// The album page's menu: shuffle and queue the album, which Play beside it
/// starts; rate it under the library's rating system; edit the release shown,
/// open its storage, export it and save it; make that release the album's
/// primary one; and delete the release or the album.
struct AlbumMenu: MacMenuPart {
    /// Whether the menu offers Set as primary release: the album holds
    /// another release, and the one shown is not its primary one.
    let offersSetAsPrimary: Bool
    /// The album's ratings under the library's rating system: Stars offers
    /// the Rate submenu, Heart the line of who hearted the album and Add
    /// heart or Remove heart, and None neither.
    let rating: BridgeAlbumRating
    /// Rate the album with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void
    /// Turn the person's heart on the album on or off.
    let onSetHearted: (Bool) -> Void
    let onShuffle: () -> Void
    let onAddNext: () -> Void
    let onAddToQueue: () -> Void
    /// Open the release editor: cover, fields, album and catalog search.
    let onEdit: () -> Void
    let onOpenStorage: () -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    let onSetPrimaryRelease: () -> Void
    let onDeleteRelease: () -> Void
    let onDeleteAlbum: () -> Void

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        MacMenuItem("Shuffle", action: onShuffle)
        MacMenuItem("Play next", action: onAddNext)
        MacMenuItem("Add to queue", action: onAddToQueue)
        MacMenuItem.divider
        if case .stars(let mine, let raters) = rating {
            RateMenu(stars: mine, raters: raters, onRate: onRate)
            MacMenuItem.divider
        }
        else if case .heart(let hearted, let hearters) = rating {
            if let hearters {
                MacMenuItem(verbatim: hearters.text, disabled: true) {}
            }
            HeartMenuItem(hearted: hearted, onSetHearted: onSetHearted)
            MacMenuItem.divider
        }
        MacMenuItem("Edit…", action: onEdit)
        MacMenuItem("Manage storage…", action: onOpenStorage)
        MacMenuItem("Export…", action: onExport)
        MacMenuItem("Save as…", action: onSaveAs)
        if offersSetAsPrimary {
            MacMenuItem.divider
            MacMenuItem("Set as primary release", action: onSetPrimaryRelease)
        }
        MacMenuItem.divider
        MacMenuItem("Delete release", action: onDeleteRelease)
        MacMenuItem("Delete album…", action: onDeleteAlbum)
    }
}
