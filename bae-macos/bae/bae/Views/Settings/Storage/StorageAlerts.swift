import BaeKit
import SwiftUI

/// The alert Settings → Storage asks before it deletes what a reset left.
@MainActor
enum StorageAlerts {
    /// Asks to delete the data from before the reset, which is `size` on this
    /// Mac. The library is not affected, and the data can't be recovered.
    static func deleteBeforeReset(size: String) -> (
        MacAlertWords, [MacAlertButton]
    ) {
        (
            MacAlertWords(
                Text("Delete data from before the reset?"),
                message: Text(
                    "\(size) will be removed from this Mac. It cannot be recovered. Your library is not affected."
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
