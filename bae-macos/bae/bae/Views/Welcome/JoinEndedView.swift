import BaeKit
import SwiftUI

/// A join that ended without letting this device in, whether the invite was
/// declined, cancelled or expired, or the device that made it is gone: one
/// step that says so and whom to ask for a new one. OK returns to the first
/// screen.
struct JoinEndedView: View {
    let inviter: String
    let onOK: () -> Void

    var body: some View {
        MacStep(
            String(localized: "This invite has ended"),
            text: String(localized: "Ask \(inviter) for a new one.")
        ) {
            MacStepActions {
                Button("OK", action: onOK)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
    }
}
