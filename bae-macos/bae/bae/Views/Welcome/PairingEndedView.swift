import BaeKit
import SwiftUI

/// A pairing that ended without letting this device in, whether it was
/// declined, expired or closed on the other device: one step that says so
/// and what to do. OK returns to the first screen.
struct PairingEndedView: View {
    let onOK: () -> Void

    var body: some View {
        MacStep(
            String(localized: "This code has ended"),
            text: String(
                localized: "Open Add device on your other device for a new one."
            )
        ) {
            MacStepActions {
                Button("OK", action: onOK)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
    }
}
