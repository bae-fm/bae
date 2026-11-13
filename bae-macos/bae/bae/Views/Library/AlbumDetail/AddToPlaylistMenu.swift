import BaeKit

/// A track menu's Add to playlist submenu: the person's own playlists by
/// name, then New playlist…, which starts one holding the track. Nothing
/// shows until the playlists are read, or without a profile to make them
/// under.
struct AddToPlaylistMenu: MacMenuPart {
    let listing: BridgePlaylistListing?
    let onAdd: (String) -> Void
    let onNewPlaylist: () -> Void

    var entries: [MacMenuEntry] {
        guard let listing, listing.canCreate else { return [] }
        var items: [MacMenuEntry] = listing.addTargets.map { playlist in
            .item(
                MacMenuItem(verbatim: playlist.name) { onAdd(playlist.id) }
            )
        }
        if !items.isEmpty {
            items.append(.divider)
        }
        items.append(.item(MacMenuItem("New playlist…") { onNewPlaylist() }))
        return [
            .submenu(String(localized: "Add to playlist"), items, nil),
            .divider,
        ]
    }
}

/// What a track's menu needs to offer Add to playlist: the playlists the
/// person can add to, and what choosing one, or New playlist…, does with the
/// track.
struct TrackPlaylistChoices {
    let listing: BridgePlaylistListing?
    let onAdd: (_ trackId: String, _ playlistId: String) -> Void
    let onNewPlaylist: (_ trackId: String) -> Void

    /// No playlists: the menu offers none.
    static var none: TrackPlaylistChoices {
        TrackPlaylistChoices(
            listing: nil,
            onAdd: { _, _ in },
            onNewPlaylist: { _ in }
        )
    }
}
