import BaeKit
import SwiftUI

/// The alert Settings → Storage asks before it deletes what a reset left.
@MainActor
enum StorageAlerts {
    /// Asks before bae deletes the old library a reset left, which takes `size`
    /// on this Mac. The library in use stays as it is, and nobody can get the
    /// deleted data back.
    static func deleteBeforeReset(size: String) -> (
        MacAlertWords, [MacAlertButton]
    ) {
        (
            MacAlertWords(
                Text("Delete the old library?"),
                message: Text(
                    "bae will delete \(size) from this Mac. You cannot get this data back. Your library stays as it is."
                )
            ),
            [
                MacAlertButton(Text("Delete"), role: .destructive),
                MacAlertButton(Text("Cancel"), role: .cancel),
            ]
        )
    }

    /// Shows `alert` and answers whether its first button was chosen.
    static func confirms(
        _ alert: (MacAlertWords, [MacAlertButton]),
        asking asker: AlertAsker
    ) async -> Bool {
        await asker.ask(alert) == 0
    }
}
