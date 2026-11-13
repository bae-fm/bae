import BaeKit
import SwiftUI

/// The confirmation to discard an unfinished join this Mac can't read: it
/// leaves this Mac, and the library in the cloud stays as it is.
@MainActor
enum InstallationDiscardAlert {
    static let confirmation: (MacAlertWords, [MacAlertButton]) = (
        MacAlertWords(
            Text("Discard the unfinished join?"),
            message: Text(
                "It did not finish, and this Mac cannot read what it saved. Discarding removes it from this Mac. The library in the cloud is not touched."
            )
        ),
        [
            MacAlertButton(Text("Discard"), role: .destructive),
            MacAlertButton(Text("Cancel"), role: .cancel),
        ]
    )
}
