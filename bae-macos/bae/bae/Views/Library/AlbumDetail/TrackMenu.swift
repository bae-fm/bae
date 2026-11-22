import BaeKit

/// A track row's menu. On a row among several picked tracks it plays,
/// queues and adds to a playlist every picked track; on any other row it
/// plays, queues, rates, adds to a playlist and saves the row's track alone.
struct TrackMenu: MacMenuPart {
    let track: Track
    /// The tracks the menu acts on: every picked track when the row is
    /// among several, else the row's track alone.
    let targets: [String]
    /// What the menu does with several picked tracks, or `nil` where the
    /// list does not let the person pick tracks.
    let picking: TrackPicking?
    let onPlay: () -> Void
    let onAddNext: (String) -> Void
    let onAddToQueue: (String) -> Void
    let onExportTrack: (String) -> Void
    /// The track's rating under the library's rating system, which the
    /// row's pending heart can replace: the menu offers Rate under stars,
    /// Add heart or Remove heart under heart, and neither under none.
    let rating: BridgeTrackRating
    /// Rate the track with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void
    /// Turn the person's heart on the track on or off.
    let onSetHearted: (Bool) -> Void
    /// The playlists the menu can add the tracks to.
    let playlists: TrackPlaylistChoices

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
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
            MacMenuItem.divider
            playlistMenu
        }
        else {
            MacMenuItem("Play") { onPlay() }
            MacMenuItem("Play next") { onAddNext(track.id) }
            MacMenuItem("Add to queue") { onAddToQueue(track.id) }
            MacMenuItem.divider
            if case .stars(let mine) = rating {
                TrackRateMenu(stars: mine, onRate: onRate)
            }
            else if case .heart(let hearted) = rating {
                HeartMenuItem(hearted: hearted, onSetHearted: onSetHearted)
            }
            playlistMenu
            MacMenuItem("Save as…") { onExportTrack(track.id) }
        }
    }
}
