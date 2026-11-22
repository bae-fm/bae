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
    /// How the library shows the ratings of albums. Only Heart puts items in
    /// the menu: stars are drawn and changed under Play's row.
    var ratingSystem = BridgeRatingSystem.stars
    /// The album's ratings, which the Heart items read and change.
    var rating = BridgeAlbumRating(mine: nil, raters: [])
    /// Rate the album with these stars, or clear its rating with none.
    var onRate: (BridgeStars?) -> Void = { _ in }
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
        if ratingSystem == .heart {
            if !rating.heartersText.isEmpty {
                MacMenuItem(verbatim: rating.heartersText, disabled: true) {}
            }
            heartItem
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

extension AlbumMenu {
    /// Heart, or Remove heart on an album the person hearted.
    private var heartItem: MacMenuItem {
        let toggle = { onRate(rating.starsAfterHeartToggle) }
        return rating.isHearted
            ? MacMenuItem("Remove heart", action: toggle)
            : MacMenuItem("Heart", action: toggle)
    }
}
