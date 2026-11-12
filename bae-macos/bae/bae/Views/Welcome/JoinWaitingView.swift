import BaeKit
import SwiftUI

/// A join running: the library's name, what it waits on, and, while it waits
/// for approval, until when it waits and its confirmation number
/// with when it checks next and Check now. Cancel shows only until the
/// request may have been sent.
struct JoinWaitingView: View {
    let form: BootstrapCodeForm
    let invite: BridgeInviteInfo

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            Spacer()
            Text(verbatim: invite.libraryName)
                .themeText(.title)
            if case .waitingForApproval = form.status {
                Text(
                    "Waiting for approval until \(invite.expiresAt.formatted(date: .abbreviated, time: .shortened))"
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
            }
            if let status = form.status {
                MacProgress(steps: form.steps) {
                    BootstrapStatusView(status: status)
                }
            }
            if let error = form.error {
                Notice(error: error)
            }
            HStack(spacing: ThemeSpace.group) {
                if form.cancellable {
                    Button("Cancel") { form.cancel() }
                        .buttonStyle(.bordered)
                }
                if case .waitingForApproval(_, let nextCheck) = form.status {
                    Button("Check now") { form.checkNow() }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(!nextCheck.endsOnCheckNow)
                }
            }
            Spacer()
        }
        .padding(.horizontal)
    }
}

extension BridgeNextCheck {
    /// Whether Check now ends this wait: the join's own backoff, but neither a
    /// check already running nor a provider's cooldown.
    fileprivate var endsOnCheckNow: Bool {
        if case .at = self { return true }
        return false
    }
}
