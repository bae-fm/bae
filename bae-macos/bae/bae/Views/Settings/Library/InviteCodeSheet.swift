import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InviteCodeSheet")

/// One of this device's open invites, shown again. While it shows, this
/// device syncs every few seconds, so the invite's request arrives soon after
/// it is sent. It closes when the invite ends.
struct InviteCodeSheet: View {
    let inviteId: String
    let onDismiss: () -> Void

    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync
    @Environment(SyncStatusStore.self)
    private var syncStatus
    @Environment(ConfigStore.self)
    private var configStore

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
        Group {
            if let invite {
                InviteCodeSheetContent(
                    invite: invite,
                    providerName: configStore.storage?.displayName ?? "",
                    offline: syncStatus.offline,
                    error: error,
                    busy: task != nil,
                    onCancelInvite: { confirmingCancel = true },
                    onDone: onDismiss
                )
            }
        }
        // An ended invite has nothing left to show.
        .onChange(of: invite == nil) { _, ended in
            if ended { onDismiss() }
        }
        .task { await store.holdInviteScreenSync() }
        .alert(
            localizedCoreString("core.invite.cancel_confirm.title"),
            isPresented: $confirmingCancel,
            presenting: invite
        ) { shown in
            Button("Cancel invite") { cancel(shown) }
            Button(
                localizedCoreString("core.invite.cancel_confirm.keep"),
                role: .cancel
            ) {}
        } message: { shown in
            if let forgetLine = shown.access.forgetLine {
                Text("\(shown.label) won’t be able to use this invite.")
                    + Text(verbatim: "\n\n\(forgetLine)")
            }
            else {
                Text("\(shown.label) won’t be able to use this invite.")
            }
        }
        .onDisappear { task?.cancel() }
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

/// The invite's sheet: while sync shares the library with the invited address,
/// a bar saying so, or what holds it; while this Mac is offline, that the code
/// shows once it is online and the library is shared; then the code to scan or
/// enter on their device. Cancel invite sits beside Done.
struct InviteCodeSheetContent: View {
    let invite: BridgeInvite
    /// The name of the storage provider the library is shared through.
    let providerName: String
    /// Whether the sync state says this Mac can't reach its storage.
    let offline: Bool
    let error: DisplayError?
    let busy: Bool
    let onCancelInvite: () -> Void
    let onDone: () -> Void

    var body: some View {
        MacSheet(
            "Invite",
            message: waitingOffline
                ? "The code shows here once this Mac is online and \(providerName) has shared the library with \(invite.label)."
                : nil,
            width: 440
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.group) {
                switch invite.shown {
                case .granting: granting
                case .code(let code, _): shown(code)
                }
                if let error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel invite", action: onCancelInvite)
                    .disabled(busy)
                Button("Done", action: onDone)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
    }

    /// Sync hasn't shared the library with the invited address, and this Mac
    /// can't reach its storage, which the sheet says in place of a bar.
    private var waitingOffline: Bool {
        guard case .granting = invite.shown else { return false }
        return offline && invite.held == nil
    }

    /// A refused check or a failed grant says why; cancelling ends the invite.
    @ViewBuilder
    private var granting: some View {
        if let held = invite.held {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                ErrorText(localizedCoreString(held.reason.key))
                if let detail = held.reason.detail {
                    Text(detail)
                        .themeText(.mono)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
        else if !offline {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text("Sharing the library with \(invite.label)…")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                ProgressView()
                    .progressViewStyle(.linear)
            }
        }
    }

    private func shown(_ code: String) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            // The person beside the admin scans it; one elsewhere gets it
            // copied or shared.
            MacScanCode(
                instruction: "Scan or enter the code on their device.",
                qr: code,
                qrLabel: "Invite",
                code: code
            )
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
        }
    }
}
