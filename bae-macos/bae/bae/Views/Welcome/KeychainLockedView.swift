import BaeKit
import SwiftUI

/// Shown while the keychain refuses the library's key because the Mac is
/// locked; `AppDelegate` retries on unlock, and the button retries by hand.
struct KeychainLockedView: View {
    let onRetry: () -> Void

    var body: some View {
        // Core's sentence for this failure, as every surface shows it.
        MacStep(
            String(localized: "Library locked"),
            text: BridgeErrorCategory.keyringLocked.localizedLine
        ) {
            MacStepActions {
                MacButton("Try again", kind: .primary, action: onRetry)
                    .keyboardShortcut(.defaultAction)
            }
        }
    }
}
