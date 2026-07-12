import Foundation

/// Cloud sync connection management and sync-config writes plus the
/// restore-code generator used by the settings UI.
public final class Sync: Sendable, Observable {
    public let signInCloudProvider:
        @Sendable (
            _ provider: BridgeCloudProvider, _ storage: BridgeHomeStorage
        )
            async throws -> Void
    public let disconnectCloudProvider: @Sendable () throws -> Void
    /// Set up iCloud (CloudKit) as the sync provider: validate the iCloud
    /// account is usable, register the CloudKit driver with the bridge, then
    /// persist the provider. Throws before persisting when iCloud is
    /// unavailable, so a signed-out account never lands `provider: CloudKit`
    /// in the config.
    public let connectCloudkit:
        @Sendable (_ storage: BridgeHomeStorage) async throws -> Void
    public let saveSyncConfig:
        @Sendable (_ configData: BridgeSaveSyncConfig) async throws -> Void
    public let generateRestoreCode: @Sendable () throws -> String
    /// The library's membership: its devices (with this device flagged) and
    /// whether the running device is an owner. Reads the membership chain from
    /// cloud storage, so it runs off the main thread.
    public let getMembers: @Sendable () async throws -> BridgeMembership
    /// Approve a joining device by its public key (from its join-request code),
    /// returning the invite code to hand back to that device.
    public let inviteMember:
        @Sendable (_ publicKeyHex: String, _ providerAccountEmail: String?)
            async throws -> String
    /// Remove a device from the library and rotate the library key.
    public let removeMember:
        @Sendable (_ publicKeyHex: String) async throws -> Void
    /// Warning text for the disconnect-sync confirmation: `nil` when no
    /// releases will become unplayable, otherwise a pre-formatted sentence
    /// the UI appends to the base "this will stop syncing" message.
    public let disconnectWarningMessage: @Sendable () async throws -> String?
    /// Retry failed cloud-outbox uploads now (clears their backoff and kicks
    /// the sync loop).
    public let retryOutbox: @Sendable () async throws -> Void
    /// Rename a library by id. If the id matches the active library the
    /// reactive `ConfigState` handles the rename; for any other local
    /// library the on-disk `config.yaml` is edited in place. The sidebar
    /// can rename either the active row or any inactive local row.
    public let renameLibrary:
        @Sendable (_ libraryId: String, _ newName: String) throws -> Void
    /// Cancel one queued outbox entry by id (dequeues it; the local file stays).
    public let cancelOutboxItem: @Sendable (_ id: Int64) async throws -> Void
    /// Cancel whatever transition a release is mid-flight — pin, upload, or
    /// unmanage — leaving it in its prior state. A no-op if nothing's running.
    public let cancelReleaseTransition:
        @Sendable (_ releaseId: String) async throws
            -> Void
    /// Pause or resume the cloud-upload pipeline. In-flight uploads finish; the
    /// queue stops draining until resumed.
    public let setSyncPaused: @Sendable (_ paused: Bool) async -> Void
    // periphery:ignore - called from the iOS pull-to-refresh / sync-retry; the
    // macOS target periphery analyzes doesn't use it (sync is automatic there).
    /// Re-kick the sync loop now (manual pull-to-refresh / retry). Non-throwing.
    public let triggerSync: @Sendable () -> Void
    /// Delete the active library's encryption key from the OS keyring.
    /// The current session keeps working (the key stays in memory);
    /// the next launch lands on the unlock screen.
    public let lockActiveLibrary: @Sendable () throws -> Void

    public init(
        signInCloudProvider:
            @escaping @Sendable (BridgeCloudProvider, BridgeHomeStorage)
            async throws ->
            Void = { _, _ in
            },
        disconnectCloudProvider: @escaping @Sendable () throws -> Void = {},
        connectCloudkit:
            @escaping @Sendable (BridgeHomeStorage) async throws -> Void = {
                _ in
            },
        saveSyncConfig:
            @escaping @Sendable (BridgeSaveSyncConfig) async throws -> Void = {
                _ in
            },
        generateRestoreCode: @escaping @Sendable () throws -> String = { "" },
        getMembers: @escaping @Sendable () async throws -> BridgeMembership = {
            throw StubError.notImplemented
        },
        inviteMember:
            @escaping @Sendable (String, String?) async throws -> String = {
                _,
                _ in
                throw StubError.notImplemented
            },
        removeMember: @escaping @Sendable (String) async throws -> Void = {
            _ in
            throw StubError.notImplemented
        },
        disconnectWarningMessage:
            @escaping @Sendable () async throws
            -> String? = { throw StubError.notImplemented },
        retryOutbox: @escaping @Sendable () async throws -> Void = {},
        renameLibrary: @escaping @Sendable (String, String) throws -> Void = {
            _,
            _ in
            throw StubError.notImplemented
        },
        cancelOutboxItem: @escaping @Sendable (Int64) async throws -> Void = {
            _ in
        },
        cancelReleaseTransition:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        setSyncPaused: @escaping @Sendable (Bool) async -> Void = { _ in },
        triggerSync: @escaping @Sendable () -> Void = {},
        lockActiveLibrary: @escaping @Sendable () throws -> Void = {
            throw StubError.notImplemented
        }
    ) {
        self.signInCloudProvider = signInCloudProvider
        self.disconnectCloudProvider = disconnectCloudProvider
        self.connectCloudkit = connectCloudkit
        self.saveSyncConfig = saveSyncConfig
        self.generateRestoreCode = generateRestoreCode
        self.getMembers = getMembers
        self.inviteMember = inviteMember
        self.removeMember = removeMember
        self.disconnectWarningMessage = disconnectWarningMessage
        self.retryOutbox = retryOutbox
        self.triggerSync = triggerSync
        self.renameLibrary = renameLibrary
        self.cancelOutboxItem = cancelOutboxItem
        self.cancelReleaseTransition = cancelReleaseTransition
        self.setSyncPaused = setSyncPaused
        self.lockActiveLibrary = lockActiveLibrary
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            // `signInCloudProvider` (OAuth) and `connectCloudkit` (iCloud) bind
            // to bridge methods that exist only when their feature is compiled
            // in. The UI that calls these is compiled out of baeium builds, so
            // there the closures are unreachable stubs.
            signInCloudProvider: { provider, storage in
                #if BAE_OAUTH_PROVIDERS
                    try await handle.signInCloudProvider(
                        provider: provider,
                        storage: storage
                    )
                #else
                    throw StubError.notImplemented
                #endif
            },
            disconnectCloudProvider: { try handle.disconnectCloudProvider() },
            connectCloudkit: { storage in
                #if BAE_CLOUDKIT
                    // The driver is installed once at app startup; here we only
                    // pre-flight the iCloud account before persisting CloudKit.
                    try await CloudKitService.bae().checkAccountAvailable()
                    try await handle.useCloudkit(storage: storage)
                #else
                    throw StubError.notImplemented
                #endif
            },
            saveSyncConfig: { try await handle.saveSyncConfig(configData: $0) },
            generateRestoreCode: { try handle.generateRestoreCode() },
            getMembers: { try await handle.getMembers() },
            inviteMember: {
                try await handle.inviteMember(
                    publicKeyHex: $0,
                    providerAccountEmail: $1
                )
            },
            removeMember: { try await handle.removeMember(publicKeyHex: $0) },
            disconnectWarningMessage: {
                try await handle.disconnectWarningMessage()
            },
            retryOutbox: { try await handle.retryOutbox() },
            renameLibrary: {
                try handle.renameLibrary(libraryId: $0, name: $1)
            },
            cancelOutboxItem: { try await handle.cancelOutboxItem(id: $0) },
            cancelReleaseTransition: {
                try await handle.cancelReleaseTransition(releaseId: $0)
            },
            setSyncPaused: { await handle.setSyncPaused(paused: $0) },
            triggerSync: { handle.triggerSync() },
            lockActiveLibrary: { try handle.lockActiveLibrary() }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static let stub = Sync()
    #endif

    /// Generate a fresh restore code and persist it to iCloud Keychain
    /// against `libraryId`. Background-detached; errors surface through
    /// `onError` on the main actor. Called on startup (via
    /// `BaeApp.openLibrary` when `isSyncReady`) and from
    /// `LibrarySettingsTab` after every successful sync-config change.
    public func storeRestoreCodeInKeychain(
        libraryId: String,
        onError: @escaping @Sendable (String) -> Void
    ) {
        Task.detached { [generateRestoreCode] in
            do {
                let code = try generateRestoreCode()
                KeychainService.saveRestoreCode(
                    libraryId: libraryId,
                    code: code
                )
            }
            catch {
                await MainActor.run {
                    onError(
                        "Failed to generate restore code: \(error.localizedDescription)"
                    )
                }
            }
        }
    }
}
