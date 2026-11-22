import BaeKit

/// The album page's menu: play and queue the album; edit the release shown,
/// open its storage, export it and save it; make that release the album's
/// primary one; and delete the release or the album. Under the Heart rating
/// system it also holds the line of who hearted the album and Heart, or
/// Remove heart on an album the person hearted.
struct AlbumMenu: MacMenuPart {
    /// Whether the menu offers Set as primary release: the album holds
    /// another release, and the one shown is not its primary one.
    let offersSetAsPrimary: Bool
    /// The album's ratings under the library's rating system. Only Heart puts
    /// items in the menu: stars are drawn and changed under Play's row.
    let rating: BridgeAlbumRating
    /// Turn the person's heart on the album on or off.
    let onSetHearted: (Bool) -> Void
    let onPlay: () -> Void
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
        MacMenuItem("Play", action: onPlay)
        MacMenuItem("Shuffle", action: onShuffle)
        MacMenuItem.divider
        MacMenuItem("Play next", action: onAddNext)
        MacMenuItem("Add to queue", action: onAddToQueue)
        MacMenuItem.divider
        if case .heart(let hearted, let hearters) = rating {
            if !hearters.isEmpty {
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
