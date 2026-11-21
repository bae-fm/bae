import BaeKit
import SwiftUI

/// The alert that asks before a release editor's save deletes tracks that
/// have a rating, a play in the history or a playlist entry: Save runs
/// `onSave`, and Cancel leaves the editor as it is.
enum DropTracksConfirmation {
    /// The alert; Save runs `onSave`.
    static func alert(
        onSave: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text("Delete track data?"),
                message: Text(
                    "Saving changes the release’s track layout, which deletes the current tracks’ ratings, play history and playlist entries."
                )
            ),
            [
                MacAlertButton(
                    Text("Save"),
                    role: .destructive,
                    action: onSave
                ),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }
}
