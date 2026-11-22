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
    /// Rate the track with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void
    /// How the library shows the ratings of tracks: the menu offers Rate
    /// under stars, Heart or Remove heart under heart, and neither under none.
    var ratingSystem = BridgeRatingSystem.stars
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
            if ratingSystem == .stars {
                TrackRateMenu(stars: track.myStars, onRate: onRate)
            }
            else if ratingSystem == .heart {
                heartItem
            }
            playlistMenu
            MacMenuItem("Save as…") { onExportTrack(track.id) }
        }
    }
}

extension TrackMenu {
    /// Heart, or Remove heart on a hearted track.
    private var heartItem: MacMenuItem {
        let toggle = {
            onRate(BridgeStars.afterHeartToggle(from: track.myStars))
        }
        return track.myStars?.isHeart == true
            ? MacMenuItem("Remove heart", action: toggle)
            : MacMenuItem("Heart", action: toggle)
    }
}
