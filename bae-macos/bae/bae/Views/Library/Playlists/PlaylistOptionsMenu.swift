import BaeKit

/// The menu of a playlist's page: Shuffle, then, on the person's own
/// playlist, edit it or delete it.
struct PlaylistOptionsMenu: MacMenuPart {
    /// What the owner of a playlist can do to it.
    struct OwnerActions {
        let edit: () -> Void
        let delete: () -> Void
    }

    let onShuffle: () -> Void
    /// Nil on a playlist someone else made, which the person cannot change.
    let owner: OwnerActions?

    @MacMenuBuilder
    var entries: [MacMenuEntry] {
        MacMenuItem("Shuffle") { onShuffle() }
        if let owner {
            MacMenuItem("Edit playlist…") { owner.edit() }
            MacMenuItem("Delete playlist…") { owner.delete() }
        }
    }
}
