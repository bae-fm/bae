import BaeKit
import SwiftUI

/// Shown while the keychain refuses the library's key because the Mac is
/// locked; `AppDelegate` retries on unlock, and the button retries by hand.
struct KeychainLockedView: View {
    let onRetry: () -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            Text("Library locked")
                .themeText(.title)
            // Core's sentence for this failure, as every surface shows it.
            Text(BridgeErrorCategory.keyringLocked.localizedLine)
                .themeText(.body)
                .foregroundStyle(Theme.macTextSecondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: WelcomeLayout.columnWidth)
            MacButton("Try again", kind: .primary, action: onRetry)
                .keyboardShortcut(.defaultAction)
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
