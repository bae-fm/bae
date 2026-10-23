import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InviteCodeSheet")

/// One of this device's open invites, shown again: the QR code of an
/// in-person invite, or the code of a sent one with Copy and Share, when it
/// expires, and Cancel Invitation. It closes when the invite ends.
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
                code(invite)
            }
        }
        .frame(width: 440, height: 520)
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
            Button("Cancel Invitation", role: .destructive) { cancel(shown) }
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

    private func code(_ invite: BridgeInvite) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            switch invite.delivery {
            case .inPerson:
                if let qrImage = QRCode.image(from: invite.code) {
                    Image(nsImage: qrImage)
                        .interpolation(.none)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 200, height: 200)
                }
            case .sent:
                Text(invite.code)
                    .themeText(.mono)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity)
                HStack(spacing: ThemeSpace.group) {
                    Button("Copy") {
                        SystemActions.copyToPasteboard(invite.code)
                    }
                    ShareLink(item: invite.code)
                }
            }
            Text(
                "Expires \(invite.expiresAt, format: .relative(presentation: .named))"
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
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
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            Button("Cancel Invitation", role: .destructive) {
                confirmingCancel = true
            }
            .disabled(task != nil)
            Spacer()
        }
        .padding()
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
                    "invite cancel failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
