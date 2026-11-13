import BaeKit
import SwiftUI

/// This device waiting to be added from another of the person's devices: the
/// number to check against the one that device shows, and the line it waits
/// on. No Cancel, since the request may be sent. Until the request is sent,
/// the progress of connecting.
struct PairingWaitingView: View {
    let status: BridgeBootstrapStatus?
    let steps: LatestSource<BridgeBootstrapStep>?
    let onCancel: () -> Void

    var body: some View {
        if case .waitingForApproval(let confirmation) = status {
            MacStep(
                String(
                    localized: "Check that your other device shows this code"
                ),
                text: String(localized: "Then add this device there.")
            ) {
                MacMatchCode(
                    code: confirmation,
                    waiting: String(
                        localized: "Waiting for your other device…"
                    )
                )
            }
        }
        else {
            CodeRunningView(
                openSteps: nil,
                steps: steps,
                status: status,
                onCancel: onCancel
            )
        }
    }
}
