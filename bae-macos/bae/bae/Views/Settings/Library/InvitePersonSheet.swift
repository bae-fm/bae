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
                MacInviteFields(
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
