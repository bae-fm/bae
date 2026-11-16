import BaeKit

/// The album page's menu: play and queue the album, edit, store, export and
/// save the release shown, make that release the album's primary one, and
/// delete the release or the album.
struct AlbumMenu: MacMenuPart {
    /// Whether the menu offers Set as primary release: the album holds
    /// another release, and the one shown is not its primary one.
    let offersSetAsPrimary: Bool
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
        MacMenuDivider()
        MacMenuItem("Play next", action: onAddNext)
        MacMenuItem("Add to queue", action: onAddToQueue)
        MacMenuDivider()
        MacMenuItem("Edit…", action: onEdit)
        MacMenuItem("Storage…", action: onOpenStorage)
        MacMenuItem("Export…", action: onExport)
        MacMenuItem("Save as…", action: onSaveAs)
        if offersSetAsPrimary {
            MacMenuDivider()
            MacMenuItem("Set as primary release", action: onSetPrimaryRelease)
        }
        MacMenuDivider()
        MacMenuItem("Delete release", action: onDeleteRelease)
        MacMenuItem("Delete album…", action: onDeleteAlbum)
    }
}
