import BaeKit

/// The menu of the person's own playlist on its page: edit the playlist, or
/// delete it.
struct PlaylistOptionsMenu: MacMenuPart {
    let onEdit: () -> Void
    let onDelete: () -> Void

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        MacMenuItem("Edit playlist…") { onEdit() }
        MacMenuItem.divider
        MacMenuItem("Delete playlist…") { onDelete() }
    }
}
