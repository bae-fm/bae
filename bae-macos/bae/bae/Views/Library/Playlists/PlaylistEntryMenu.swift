import BaeKit

/// A playlist entry's menu: play the entry, queue it, and remove it from the
/// playlist where the person can.
struct PlaylistEntryMenu: MacMenuPart {
    let play: () -> Void
    let playNext: () -> Void
    let addToQueue: () -> Void
    /// Remove the entry from the playlist; `nil` for a playlist the person
    /// can't change, whose menu has no Remove from playlist.
    let remove: (() -> Void)?

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        MacMenuItem("Play") { play() }
        MacMenuItem("Play next") { playNext() }
        MacMenuItem("Add to queue") { addToQueue() }
        if let remove {
            MacMenuItem.divider
            MacMenuItem("Remove from playlist") { remove() }
        }
    }
}
