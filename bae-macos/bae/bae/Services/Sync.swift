import Foundation

/// Cloud sync connection management and sync-config writes plus the
/// restore-code generator used by the settings UI.
final class Sync: Sendable, Observable {
    let signInCloudProvider:
        @Sendable (
            _ provider: BridgeCloudProvider, _ storage: BridgeHomeStorage
        )
            async throws -> Void
    let disconnectCloudProvider: @Sendable () throws -> Void
    /// Set up iCloud (CloudKit) as the sync provider: validate the iCloud
    /// account is usable, register the CloudKit driver with the bridge, then
    /// persist the provider. Throws before persisting when iCloud is
    /// unavailable, so a signed-out account never lands `provider: CloudKit`
    /// in the config.
    let connectCloudkit:
        @Sendable (_ storage: BridgeHomeStorage) async throws -> Void
    let saveSyncConfig:
        @Sendable (_ configData: BridgeSaveSyncConfig) async throws -> Void
    let generateRestoreCode: @Sendable () throws -> String
    /// Warning text for the disconnect-sync confirmation: `nil` when no
    /// releases will become unplayable, otherwise a pre-formatted sentence
    /// the UI appends to the base "this will stop syncing" message.
    let disconnectWarningMessage: @Sendable () throws -> String?
    /// Retry failed cloud-outbox uploads now (clears their backoff and kicks
    /// the sync loop).
    let retryOutbox: @Sendable () throws -> Void
    /// Rename a library by id. If the id matches the active library the
    /// reactive `ConfigState` handles the rename; for any other local
    /// library the on-disk `config.yaml` is edited in place. The sidebar
    /// can rename either the active row or any inactive local row.
    let renameLibrary:
        @Sendable (_ libraryId: String, _ newName: String) throws -> Void
    /// Cancel one queued outbox entry by id (dequeues it; the local file stays).
    let cancelOutboxItem: @Sendable (_ id: Int64) throws -> Void
    /// Stop uploading a release and keep it local-only: drops its queued and
    /// in-flight uploads and deletes any blobs already uploaded this attempt.
    let cancelReleaseUpload: @Sendable (_ releaseId: String) throws -> Void
    /// Pause or resume the cloud-upload pipeline. In-flight uploads finish; the
    /// queue stops draining until resumed.
    let setSyncPaused: @Sendable (_ paused: Bool) -> Void
    // periphery:ignore - called from the iOS pull-to-refresh / sync-retry; the
    // macOS target periphery analyzes doesn't use it (sync is automatic there).
    /// Re-kick the sync loop now (manual pull-to-refresh / retry). Non-throwing.
    let triggerSync: @Sendable () -> Void
    /// Delete the active library's encryption key from the OS keyring.
    /// The current session keeps working (the key stays in memory);
    /// the next launch lands on the unlock screen.
    let lockActiveLibrary: @Sendable () throws -> Void

    init(
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
        disconnectWarningMessage: @escaping @Sendable () throws -> String? = {
            nil
        },
        retryOutbox: @escaping @Sendable () throws -> Void = {},
        renameLibrary: @escaping @Sendable (String, String) throws -> Void = {
            _,
            _ in
            throw StubError.notImplemented
        },
        cancelOutboxItem: @escaping @Sendable (Int64) throws -> Void = { _ in },
        cancelReleaseUpload: @escaping @Sendable (String) throws -> Void = {
            _ in
        },
        setSyncPaused: @escaping @Sendable (Bool) -> Void = { _ in },
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
        self.disconnectWarningMessage = disconnectWarningMessage
        self.retryOutbox = retryOutbox
        self.triggerSync = triggerSync
        self.renameLibrary = renameLibrary
        self.cancelOutboxItem = cancelOutboxItem
        self.cancelReleaseUpload = cancelReleaseUpload
        self.setSyncPaused = setSyncPaused
        self.lockActiveLibrary = lockActiveLibrary
    }

    convenience init(handle: any AppHandleProtocol) {
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
            disconnectWarningMessage: { try handle.disconnectWarningMessage() },
            retryOutbox: { try handle.retryOutbox() },
            renameLibrary: {
                try handle.renameLibrary(libraryId: $0, name: $1)
            },
            cancelOutboxItem: { try handle.cancelOutboxItem(id: $0) },
            cancelReleaseUpload: {
                try handle.cancelReleaseUpload(releaseId: $0)
            },
            setSyncPaused: { handle.setSyncPaused(paused: $0) },
            triggerSync: { handle.triggerSync() },
            lockActiveLibrary: { try handle.lockActiveLibrary() }
        )
    }

    // periphery:ignore
    static let stub = Sync()

    /// Generate a fresh restore code and persist it to iCloud Keychain
    /// against `libraryId`. Background-detached; errors surface through
    /// `onError` on the main actor. Called on startup (via
    /// `BaeApp.openLibrary` when `isSyncReady`) and from
    /// `LibrarySettingsTab` after every successful sync-config change.
    func storeRestoreCodeInKeychain(
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
