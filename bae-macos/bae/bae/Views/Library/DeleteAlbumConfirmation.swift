import BaeKit
import SwiftUI

/// The alert that asks before an album is deleted: what goes with it, what
/// stays, and that it can't be undone. It shows while `albumId` names an
/// album; Delete hands that album to `onDelete`, and either button clears
/// `albumId`.
struct DeleteAlbumConfirmation: ViewModifier {
    @Binding
    var albumId: String?
    let onDelete: (String) -> Void

    func body(content: Content) -> some View {
        content.macAlert(item: $albumId) { albumId in
            Self.alert { onDelete(albumId) }
        }
    }

    /// The alert; Delete runs `onDelete`.
    static func alert(
        onDelete: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Delete album"),
                message: Text(
                    "Every release of this album will be permanently deleted from your devices and the cloud. Original files in your folders will not be touched."
                )
            ),
            [
                MacAlertButton(
                    Text("Delete"),
                    role: .destructive,
                    action: onDelete
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}

/// The alert that asks before one release of an album is deleted.
enum DeleteReleaseConfirmation {
    /// The alert; Delete runs `onDelete`.
    static func alert(
        onDelete: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Delete release"),
                message: Text(
                    "This release will be permanently deleted from your devices and the cloud. Original files in your folders will not be touched."
                )
            ),
            [
                MacAlertButton(
                    Text("Delete"),
                    role: .destructive,
                    action: onDelete
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}

extension View {
    /// Ask before deleting the album `albumId` names; see
    /// `DeleteAlbumConfirmation`.
    func deleteAlbumConfirmation(
        albumId: Binding<String?>,
        onDelete: @escaping (String) -> Void
    ) -> some View {
        modifier(DeleteAlbumConfirmation(albumId: albumId, onDelete: onDelete))
    }
}
