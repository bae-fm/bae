import BaeKit
import SwiftUI

/// The alert that asks before a playlist is deleted: that it goes from every
/// device, and that its tracks stay in the library.
enum DeletePlaylistConfirmation {
    /// The alert for the playlist named `name`; Delete runs `onDelete`.
    static func alert(
        name: String,
        onDelete: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Delete \(name)?"),
                message: Text(
                    "It is deleted from every device. The tracks in it stay in the library."
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
