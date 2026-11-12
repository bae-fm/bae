import BaeKit
import SwiftUI

/// A join running: the library's name, what it waits on, and, while it waits
/// for approval, until when it waits and its confirmation number. While it
/// shows, the join checks for approval every few seconds. Cancel shows only
/// until the request may have been sent.
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
            }
            Spacer()
        }
        .padding(.horizontal)
        .onAppear { form.setWaitingScreenShown(true) }
        .onDisappear { form.setWaitingScreenShown(false) }
    }
}
