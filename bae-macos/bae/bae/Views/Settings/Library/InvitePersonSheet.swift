import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InvitePersonSheet")

/// An admin invites a person: their name, role and how they reach storage,
/// and whether the invite is shown in person or sent, and for how long.
/// `onCreated` receives the new invite's id, for People to show its code.
struct InvitePersonSheet: View {
    let onCreated: (String) -> Void
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore

    /// How the invite reaches the person, as the form chooses it.
    private enum Delivery: Hashable {
        case inPerson
        case send
    }

    @State
    private var name = ""
    @State
    private var role: BridgeMemberRole = .member
    @State
    private var email = ""
    @State
    private var accessKeyId = ""
    @State
    private var secretAccessKey = ""
    @State
    private var delivery: Delivery = .inPerson
    @State
    private var lifetime: BridgeInviteLifetime = .day
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

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespaces)
    }

    private var ready: Bool {
        guard task == nil, !trimmedName.isEmpty else { return false }
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
            form
        }
        .frame(width: 440, height: 520)
        .onDisappear { task?.cancel() }
    }

    private var form: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    TextField("Name", text: $name)
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
                }
                Section {
                    Picker(selection: $delivery) {
                        Text("In Person").tag(Delivery.inPerson)
                        Text("Send").tag(Delivery.send)
                    } label: {
                        EmptyView()
                    }
                    .pickerStyle(.segmented)
                    // Present in either choice so the form keeps its size.
                    Picker(selection: $lifetime) {
                        Text("1 hour").tag(BridgeInviteLifetime.hour)
                        Text("1 day").tag(BridgeInviteLifetime.day)
                        Text("1 week").tag(BridgeInviteLifetime.week)
                    } label: {
                        EmptyView()
                    }
                    .pickerStyle(.segmented)
                    .opacity(delivery == .send ? 1 : 0)
                    .allowsHitTesting(delivery == .send)
                    .accessibilityHidden(delivery != .send)
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
        let chosen: BridgeInviteDelivery =
            switch delivery {
            case .inPerson: .inPerson
            case .send: .sent(lifetime: lifetime)
            }
        let role = role
        let name = trimmedName
        error = nil
        task = Task {
            defer { task = nil }
            do {
                let invite = try await sync.invitePerson(
                    role,
                    name,
                    access,
                    chosen
                )
                onCreated(invite.id)
            }
            catch is CancellationError {
                logger.debug("invite creation cancelled")
            }
            catch {
                logger.error(
                    "invite creation failed: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
