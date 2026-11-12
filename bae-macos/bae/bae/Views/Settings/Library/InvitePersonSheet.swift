import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("InvitePersonSheet")

/// What the invited person reaches the library's storage with: the account
/// they sign in to, named for the provider, or on S3 a key.
enum InviteStorage {
    case dropbox
    case oneDrive
    case iCloud
    case s3

    /// The storage the library is on.
    init(_ storage: BridgeStorageLocation) {
        switch storage {
        case .dropbox: self = .dropbox
        case .oneDrive: self = .oneDrive
        case .cloudKit: self = .iCloud
        case .s3: self = .s3
        }
    }
}

/// An admin invites a person: the account they share the library with, or on
/// S3 a key made for them or this device's own, the role they join in and how
/// long the invite lasts. The invited person types their own name when they
/// join. `onCreated` receives the new invite's id, for People to show its
/// code.
struct InvitePersonSheet: View {
    let onCreated: (String) -> Void
    let onDismiss: () -> Void

    @Environment(Sync.self)
    private var sync
    @Environment(ConfigStore.self)
    private var configStore

    @State
    private var email = ""
    @State
    private var accessKeyId = ""
    @State
    private var secretAccessKey = ""
    @State
    private var usesOwnKey = false
    @State
    private var role = BridgeMemberRole.member
    @State
    private var lifetime = BridgeInviteLifetime.sevenDays
    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    /// Where the library is stored; none until it is set up, which an invite
    /// needs.
    private var storage: InviteStorage? {
        configStore.storage.map(InviteStorage.init)
    }

    private var access: BridgeInviteAccess? {
        switch storage {
        case nil:
            return nil
        case .s3:
            if usesOwnKey { return .heldS3AccessKey }
            if accessKeyId.isEmpty || secretAccessKey.isEmpty { return nil }
            return .s3AccessKey(
                accessKeyId: accessKeyId,
                secretAccessKey: secretAccessKey
            )
        case .dropbox, .oneDrive, .iCloud:
            let trimmed = email.trimmingCharacters(in: .whitespaces)
            return trimmed.isEmpty ? nil : .providerAccount(email: trimmed)
        }
    }

    var body: some View {
        Group {
            if let storage {
                InvitePersonForm(
                    storage: storage,
                    email: $email,
                    accessKeyId: $accessKeyId,
                    secretAccessKey: $secretAccessKey,
                    usesOwnKey: $usesOwnKey,
                    role: $role,
                    lifetime: $lifetime,
                    error: error,
                    canCreate: access != nil && task == nil,
                    onCreate: create,
                    onCancel: onDismiss
                )
            }
        }
        .onDisappear { task?.cancel() }
    }

    private func create() {
        guard let access else { return }
        error = nil
        task = Task {
            defer { task = nil }
            do {
                onCreated(try await sync.invitePerson(role, access, lifetime))
            }
            catch is CancellationError {
                logger.debug("invite creation cancelled")
            }
            catch {
                logger.error(
                    "invite creation failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// The invite form: one field for the account email, or on S3 the two key
/// fields with a line saying why, the role and how long the invite lasts, and
/// Create invite. On S3 a button beside
/// the footer switches between a key made for the invited person and this
/// device's own.
struct InvitePersonForm: View {
    let storage: InviteStorage
    @Binding
    var email: String
    @Binding
    var accessKeyId: String
    @Binding
    var secretAccessKey: String
    @Binding
    var usesOwnKey: Bool
    @Binding
    var role: BridgeMemberRole
    @Binding
    var lifetime: BridgeInviteLifetime
    let error: DisplayError?
    let canCreate: Bool
    let onCreate: () -> Void
    let onCancel: () -> Void

    @FocusState
    private var focused: Bool

    var body: some View {
        MacSheet("Invite a person", width: 440) {
            VStack(
                alignment: .leading,
                spacing: ThemeSpace.related + ThemeSpace.line
            ) {
                fields
                choices
                if let error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter(
                lead: {
                    if storage == .s3 {
                        if usesOwnKey {
                            Button("Use their own key instead") {
                                usesOwnKey = false
                            }
                        }
                        else {
                            Button("Use my key instead") { usesOwnKey = true }
                        }
                    }
                },
                trailing: {
                    Button("Cancel", action: onCancel)
                        .keyboardShortcut(.cancelAction)
                    Button("Create invite", action: onCreate)
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                        .disabled(!canCreate)
                }
            )
        }
        .onAppear { focused = true }
    }

    @ViewBuilder
    private var fields: some View {
        switch storage {
        case .dropbox:
            emailField(
                "Dropbox email",
                why:
                    "The library is shared with this Dropbox account so they can join."
            )
        case .oneDrive:
            emailField(
                "Microsoft account email",
                why:
                    "The library is shared with this Microsoft account so they can join."
            )
        case .iCloud:
            emailField(
                "Apple Account email",
                why:
                    "The library is shared with this Apple Account so they can join."
            )
        case .s3:
            if usesOwnKey {
                MacNote(
                    "They will use your key. To remove them later, you make a new key for yourself and enter it on each of your devices."
                )
            }
            else {
                FieldBox {
                    FieldBoxRow("Access key ID", labelWidth: labelWidth) {
                        TextField("Access key ID", text: $accessKeyId)
                            .labelsHidden()
                            .boxedTextField()
                            .focused($focused)
                    }
                    FieldBoxDivider()
                    FieldBoxRow(
                        "Secret access key",
                        labelWidth: labelWidth,
                        note: "Their own key to the bucket, so they can join."
                    ) {
                        SecureField("Secret access key", text: $secretAccessKey)
                            .labelsHidden()
                            .boxedTextField()
                    }
                }
            }
        }
    }

    /// The width of the labels' column: the S3 labels are the shorter.
    private var labelWidth: CGFloat { storage == .s3 ? 120 : 150 }

    private var choices: some View {
        let roles: [BridgeMemberRole] = [.member, .admin]
        let lifetimes: [BridgeInviteLifetime] = [.sevenDays, .thirtyDays]
        return FieldBox {
            FieldBoxRow("Role", labelWidth: labelWidth) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: { roles.firstIndex(of: role) ?? 0 },
                        set: { role = roles[$0] }
                    ),
                    segments: [
                        .init(
                            label: String(localized: "Member"),
                            systemImage: nil
                        ),
                        .init(
                            label: String(localized: "Admin"),
                            systemImage: nil
                        ),
                    ]
                )
            }
            FieldBoxDivider()
            FieldBoxRow("Invite lasts", labelWidth: labelWidth) {
                MacSegmentedControl(
                    selectedIndex: Binding(
                        get: { lifetimes.firstIndex(of: lifetime) ?? 0 },
                        set: { lifetime = lifetimes[$0] }
                    ),
                    segments: [
                        .init(
                            label: String(localized: "7 days"),
                            systemImage: nil
                        ),
                        .init(
                            label: String(localized: "30 days"),
                            systemImage: nil
                        ),
                    ]
                )
            }
        }
    }

    private func emailField(
        _ label: LocalizedStringKey,
        why: LocalizedStringKey
    ) -> some View {
        FieldBox {
            FieldBoxRow(label, labelWidth: labelWidth, note: why) {
                TextField(label, text: $email)
                    .labelsHidden()
                    .boxedTextField()
                    .textContentType(.emailAddress)
                    .focused($focused)
            }
        }
    }
}
