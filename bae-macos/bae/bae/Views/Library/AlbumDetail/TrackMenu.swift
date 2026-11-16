import BaeKit

/// A track row's menu. On a row among several picked tracks it plays,
/// queues and adds to a playlist every picked track; on any other row it
/// plays, queues, rates, adds to a playlist and saves the row's track alone.
struct TrackMenu: MacMenuPart {
    let track: Track
    /// Picking several tracks, or `nil` where the list has none.
    let picking: TrackPicking?
    let onPlay: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void
    /// Rate the track with these stars, or clear its rating with none; `nil`
    /// for a person who can't rate, whose menu has no Rate.
    let onRate: ((BridgeStars?) -> Void)?
    /// The playlists the menu can add the tracks to.
    let playlists: TrackPlaylistChoices

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        let targets = picking?.picked.targets(for: track.id) ?? [track.id]
        let playlistMenu = AddToPlaylistMenu(
            listing: playlists.listing,
            onAdd: { playlists.onAdd(targets, $0) },
            onNewPlaylist: { playlists.onNewPlaylist(targets) }
        )
        if let picking, targets.count > 1 {
            MacMenuItem("Play \(targets.count) tracks") {
                picking.onPlay(targets)
            }
            MacMenuItem("Play next") { picking.onAddNext(targets) }
            MacMenuItem("Add to queue") { picking.onAddToQueue(targets) }
            MacMenuDivider()
            playlistMenu
        }
        else {
            MacMenuItem("Play") { onPlay() }
            MacMenuItem("Play next") { onAddNext(track.id) }
            MacMenuItem("Add to queue") { onAddToQueue(track.id) }
            MacMenuDivider()
            if let onRate {
                TrackRateMenu(stars: track.myStars, onRate: onRate)
            }
            playlistMenu
            MacMenuItem("Save as…") { onExportTrack(track.id) }
        }
    }
}
