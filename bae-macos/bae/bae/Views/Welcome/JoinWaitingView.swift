import BaeKit
import SwiftUI

/// A join running. While it waits for approval, a step that says whom to tell
/// the code to, over the confirmation number and the line it waits on; no
/// date, no countdown, and no Cancel, since its request may be sent. In every
/// other state the library's name over what the join is doing, with Cancel
/// until the request may have been sent.
struct JoinWaitingView: View {
    let invite: BridgeInviteInfo
    let status: BridgeBootstrapStatus?
    let steps: LatestSource<BridgeBootstrapStep>?
    let error: DisplayError?
    let cancellable: Bool
    let onCancel: () -> Void

    var body: some View {
        if case .waitingForApproval(let confirmation) = status {
            MacStep(String(localized: "Tell \(invite.inviter) this code")) {
                MacMatchCode(
                    code: confirmation,
                    waiting: String(
                        localized:
                            "Waiting for \(invite.inviter) to let you in…"
                    )
                )
            }
        }
        else {
            MacStep(invite.libraryName) {
                VStack(spacing: ThemeSpace.section) {
                    if let status {
                        MacProgressContainer(steps: steps) {
                            BootstrapStatusView(status: status)
                        }
                    }
                    if let error {
                        Notice(error: error)
                    }
                    if cancellable {
                        MacStepActions {
                            Button("Cancel", action: onCancel)
                                .buttonStyle(.accentBordered)
                        }
                    }
                }
            }
        }
    }
}
