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
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?
    @State
    private var confirmingCancel = false

    /// The invite as its read last answered, once it has answered for this
    /// sheet's invite.
    private var read: Shown<String, BridgeInvite?>? {
        store.invite.shown.flatMap { $0.key == inviteId ? $0 : nil }
    }

    private var invite: BridgeInvite? { read?.value }

    var body: some View {
        // A stack, not a group: it is on screen before the read answers, so
        // its appearance starts the read.
        ZStack {
            if let invite, let storage = configStore.storage {
                InviteCodeSheetContent(
                    invite: invite,
                    providerName: storage.displayName,
                    offline: syncStatus.offline,
                    error: error,
                    busy: task != nil,
                    onCancelInvite: { confirmingCancel = true },
                    onDone: onDismiss
                )
            }
        }
        // An ended invite has nothing left to show: its read answers none.
        .onChange(of: read != nil && invite == nil) { _, ended in
            if ended { onDismiss() }
        }
        // A library with an invite always has a location. One without is a
        // bug: report it, and close the sheet instead of showing it empty.
        .onChange(of: invite != nil && configStore.storage == nil) {
            _,
            missing in
            guard missing else { return }
            let detail = "a library with an open invite has no storage location"
            logger.error("\(detail)")
            CoreBugReporter()
                .report(operation: "show an invite", detail: detail)
            uiStore.showError(
                DisplayError(
                    line: localizedCoreString("core.error.category.internal")
                )
            )
            onDismiss()
        }
        .onAppear { store.showInvite(inviteId) }
        .task { await store.holdInviteScreenSync() }
        .macAlert(
            item: Binding(
                get: { confirmingCancel ? invite : nil },
                set: { if $0 == nil { confirmingCancel = false } }
            )
        ) { shown in
            InviteCancelAlert.alert(for: shown) { cancel(shown) }
        }
        .onDisappear {
            task?.cancel()
            store.hideInvite()
        }
    }

    /// A cancelled invite's read answers none, which closes the sheet.
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
            // The sheet's own line says what it waits on, so nothing sits
            // between that line and the buttons.
            if !waitingOffline || error != nil {
                VStack(alignment: .leading, spacing: ThemeSpace.group) {
                    switch invite.shown {
                    case .granting: granting
                    case .code(let code, _): shown(code)
                    }
                    if let error {
                        MacNotice(error: error)
                    }
                }
            }
        } footer: {
            MacSheetFooter {
                MacButton("Cancel invite", action: onCancelInvite)
                    .disabled(busy)
                MacButton("Done", kind: .primary, action: onDone)
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
                MacErrorLine(localizedCoreString(held.reason.key))
                if let detail = held.reason.detail {
                    Text(detail)
                        .themeText(.mono)
                        .foregroundStyle(Theme.textSecondary)
                        .textSelection(.enabled)
                }
            }
        }
        else if !offline {
            MacProgress(
                title: String(
                    localized: "Sharing the library with \(invite.label)…"
                ),
                bar: ProgressTrackBar(progress: nil)
            )
        }
    }

    private func shown(_ code: String) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            // The person beside the admin scans it; one elsewhere gets it
            // copied or shared.
            MacScanCode(
                instruction: "Scan or enter the code on their device.",
                qr: code,
                code: code
            )
            if let keyLine = invite.access.keyLine,
                let unusedLine = invite.access.unusedInviteLine
            {
                Text(verbatim: keyLine)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
                    .textSelection(.enabled)
                Text(verbatim: unusedLine)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
                    .multilineTextAlignment(.center)
            }
        }
    }
}

/// The alert that asks before an invite is cancelled.
enum InviteCancelAlert {
    /// The alert for `invite`; Cancel invite runs `onCancel`.
    static func alert(
        for invite: BridgeInvite,
        onCancel: @escaping @MainActor () -> Void
    ) -> (MacAlertWords, [MacAlertButton]) {
        (
            MacAlertWords(
                Text(
                    verbatim: localizedCoreString(
                        "core.invite.cancel_confirm.title"
                    )
                ),
                message: message(for: invite)
            ),
            [
                MacAlertButton(Text("Cancel invite"), action: onCancel),
                MacAlertButton(
                    Text(
                        verbatim: localizedCoreString(
                            "core.invite.cancel_confirm.keep"
                        )
                    ),
                    role: .cancel
                ),
            ]
        )
    }

    private static func message(for invite: BridgeInvite) -> Text {
        let line = Text("\(invite.label) will not be able to use this invite.")
        guard let forgetLine = invite.access.forgetLine else { return line }
        return line + Text(verbatim: "\n\n\(forgetLine)")
    }
}
