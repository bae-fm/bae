import BaeKit
import SwiftUI

/// Shown when the OS keychain refused the library's key because the Mac was
/// locked or its display asleep.
///
/// Deliberately neither of the two screens this used to land on: the welcome
/// chooser, which offers to restore a library that is perfectly intact, and
/// `UnlockView`, whose copy states the key "is not in the keyring" and gates its
/// button on 64 hex characters the user does not have and should not need. The
/// key is present; this device just cannot read it this second.
///
/// There is nothing to do here, so the screen says so and waits. The retry is
/// automatic — `AppDelegate` reopens on screen unlock, session activation, and
/// app activation — and the button is for the case where the observers were not
/// the thing that changed.
struct KeychainLockedView: View {
    let onRetry: () -> Void

    var body: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "lock.fill")
                .font(.system(size: 40))
                .foregroundStyle(.secondary)
            Text("Library Locked")
                .font(.title2.bold())
            // Core owns this sentence: it is the same failure the bridge names,
            // so it reads the same here as it would in any other surface.
            Text(BridgeErrorCategory.keyringLocked.localizedLine)
                .font(.callout)
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
