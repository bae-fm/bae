import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InviteCodeSheet")

/// One of this device's open invites, shown again: while sync grants its
/// access, that it is doing so, and why coven holds it when it does; then
/// the QR code with Copy and Share; and Cancel invitation. It closes when the
/// invite ends.
struct InviteCodeSheet: View {
    let inviteId: String
    let onDismiss: () -> Void

    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?
    @State
    private var confirmingCancel = false

    private var invite: BridgeInvite? {
        store.invites.first { $0.id == inviteId }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(verbatim: invite?.invitee ?? "")
                    .themeText(.heading)
                Spacer()
                Button("Done", action: onDismiss)
                    .buttonStyle(.borderless)
            }
            .padding()
            Divider()
            if let invite {
                switch invite.shown {
                case .granting: granting(invite)
                case .code(let code, _): shown(invite, code: code)
                }
            }
        }
        .frame(width: 440, height: 640)
        // An ended invite has nothing left to show.
        .onChange(of: invite == nil) { _, ended in
            if ended { onDismiss() }
        }
        .confirmationDialog(
            localizedCoreString("core.invite.cancel_confirm.title"),
            isPresented: $confirmingCancel,
            titleVisibility: .visible,
            presenting: invite
        ) { shown in
            Button("Cancel invitation", role: .destructive) { cancel(shown) }
            Button(
                localizedCoreString("core.invite.cancel_confirm.keep"),
                role: .cancel
            ) {}
        } message: { shown in
            if let forgetLine = shown.access.forgetLine {
                Text(verbatim: forgetLine)
            }
        }
        .onDisappear { task?.cancel() }
    }

    /// Sync hasn't granted the invite's access yet: the code comes once it
    /// has. A refused check or a failed grant says why; cancelling ends it.
    private func granting(_ invite: BridgeInvite) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            if let held = invite.held {
                ErrorText(localizedCoreString(held.reason.key))
                if let detail = held.reason.detail {
                    Text(detail)
                        .themeText(.mono)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
            else {
                WaitingIndicator(
                    "Sharing the library. The code shows once this library syncs."
                )
            }
            cancelControls
            Spacer()
        }
        .padding()
    }

    private func shown(_ invite: BridgeInvite, code: String) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            // The person beside the admin scans it; one elsewhere gets it
            // copied or shared.
            QRCode(text: code)
            HStack(spacing: ThemeSpace.group) {
                Button("Copy") {
                    SystemActions.copyToPasteboard(code)
                }
                ShareLink(item: code)
            }
            if let keyLine = invite.access.keyLine,
                let unusedLine = invite.access.unusedInviteLine
            {
                Text(verbatim: keyLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                Text(verbatim: unusedLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            cancelControls
            Spacer()
        }
        .padding()
    }

    @ViewBuilder
    private var cancelControls: some View {
        if let error {
            Notice(error: error)
        }
        Button("Cancel invitation", role: .destructive) {
            confirmingCancel = true
        }
        .disabled(task != nil)
    }

    /// A cancelled invite leaves the list, which closes the sheet.
    private func cancel(_ invite: BridgeInvite) {
        error = nil
        task = Task {
            defer { task = nil }
            do { try await sync.cancelInvite(invite.id) }
            catch is CancellationError {
                logger.debug("invite cancel cancelled")
            }
            catch {
                logger.error(
                    "invite cancel failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
