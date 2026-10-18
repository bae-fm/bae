import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InvitePersonSheet")

/// An admin invites a person: their role and how they reach storage, then the
/// invitation's code for their device. The invitation carries the library's
/// current name and expires within a day; their device's join request comes
/// back here for approval.
struct InvitePersonSheet: View {
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var role: BridgeMemberRole = .member
    @State
    private var email = ""
    @State
    private var accessKeyId = ""
    @State
    private var secretAccessKey = ""
    @State
    private var invite: BridgeInvite?
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    /// An S3 invitee reaches the bucket with an access key the admin made for
    /// them; every other provider shares with their account.
    private var usesAccessKey: Bool {
        if case .s3 = configStore.storage { return true }
        return false
    }

    private var ready: Bool {
        guard task == nil else { return false }
        if usesAccessKey {
            return !accessKeyId.isEmpty && !secretAccessKey.isEmpty
        }
        return !email.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Invite a Person")
                    .themeText(.heading)
                Spacer()
                Button("Done", action: onDismiss)
                    .buttonStyle(.borderless)
            }
            .padding()
            Divider()
            if let invite {
                created(invite)
            }
            else {
                form
            }
        }
        .frame(width: 440, height: 520)
        .onDisappear { task?.cancel() }
    }

    private var form: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    Picker("Role", selection: $role) {
                        Text("Member").tag(BridgeMemberRole.member)
                        Text("Admin").tag(BridgeMemberRole.admin)
                    }
                }
                Section {
                    if usesAccessKey {
                        TextField("Access Key", text: $accessKeyId)
                        SecureField("Secret Key", text: $secretAccessKey)
                    }
                    else {
                        TextField("Their account email", text: $email)
                            .textContentType(.emailAddress)
                    }
                } footer: {
                    Text(
                        usesAccessKey
                            ? "Create an access key for them in your storage provider's console. The invitation carries it to their device."
                            : "The library's storage is shared with this account."
                    )
                }
            }
            .formStyle(.grouped)
            if let error {
                ErrorDetailDisclosure(error: error)
                    .padding(.horizontal)
            }
            HStack {
                Spacer()
                Button("Create Invitation") { create() }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ready)
            }
            .padding()
        }
    }

    private func created(_ invite: BridgeInvite) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            Spacer()
            CodeDisplay(code: invite.code, qrText: invite.code, qrSize: 200)
            Text(
                "Expires \(invite.expiresAt, format: .relative(presentation: .named))"
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            Text(
                "Their device scans or pastes this code, then asks to join. Approve it under Waiting to join."
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            Button("Cancel Invitation", role: .destructive) {
                cancel(invite)
            }
            .disabled(task != nil)
            Spacer()
        }
        .padding()
    }

    private func create() {
        let access: BridgeInviteAccess =
            usesAccessKey
            ? .s3AccessKey(
                accessKeyId: accessKeyId,
                secretAccessKey: secretAccessKey
            )
            : .providerAccount(
                email: email.trimmingCharacters(in: .whitespaces)
            )
        let role = role
        run { invite = try await sync.invitePerson(role, access) }
    }

    private func cancel(_ invite: BridgeInvite) {
        run {
            try await sync.cancelInvite(invite.id)
            onDismiss()
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) {
        error = nil
        task = Task {
            defer { task = nil }
            do { try await operation() }
            catch is CancellationError {
                logger.debug("invitation change cancelled")
            }
            catch {
                logger.error(
                    "invitation change failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
