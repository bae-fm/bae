import BaeKit

/// The menu of a playlist's page: Shuffle, then, on the person's own
/// playlist, edit it or delete it.
struct PlaylistOptionsMenu: MacMenuPart {
    let onShuffle: () -> Void
    /// Nil on a playlist someone else made, which the person cannot change.
    let onEdit: (() -> Void)?
    let onDelete: (() -> Void)?

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        MacMenuItem("Shuffle") { onShuffle() }
        if let onEdit {
            MacMenuItem("Edit playlist…") { onEdit() }
        }
        if let onDelete {
            MacMenuItem("Delete playlist…") { onDelete() }
        }
    }
}
