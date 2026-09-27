import BaeKit
import SwiftUI

/// Shown while the keychain refuses the library's key because the Mac is
/// locked; `AppDelegate` retries on unlock, and the button retries by hand.
struct KeychainLockedView: View {
    let onRetry: () -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            Image(systemName: "lock.fill")
                .themeIcon(.hero)
                .foregroundStyle(.secondary)
            Text("Library Locked")
                .themeText(.title)
            // Core's sentence for this failure, as every surface shows it.
            Text(BridgeErrorCategory.keyringLocked.localizedLine)
                .themeText(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: WelcomeLayout.columnWidth)
            Button("Try again", action: onRetry)
                .buttonStyle(PrimaryButtonStyle())
                .keyboardShortcut(.defaultAction)
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if DEBUG
    #Preview("Keychain locked") {
        WelcomeWindowChrome {
            KeychainLockedView(onRetry: {})
        }
    }
#endif
