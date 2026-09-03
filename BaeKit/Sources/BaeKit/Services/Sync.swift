import Foundation

/// Cloud sync connection management and sync-config writes plus the
/// restore-code generator used by the settings UI.
public final class Sync: Sendable, Observable {
    public let disconnectCloudProvider: @Sendable () async throws -> Void
    public let saveSyncConfig:
        @Sendable (_ configData: BridgeSaveSyncConfig) async throws -> Void
    public let generateRestoreCode: @Sendable () async throws -> String
    /// The library's membership: its devices (with this device flagged) and
    /// whether the running device is an owner. Reads the membership chain from
    /// cloud storage, so it runs off the main thread.
    public let getMembers: @Sendable () async throws -> BridgeMembership
    /// Start the owner-side local pairing session whose one code the joining
    /// device scans.
    public let startDevicePairing:
        @Sendable () async throws -> BridgeDevicePairingSession
    /// Remove a device from the library and rotate the library key.
    public let removeMember:
        @Sendable (_ publicKeyHex: String) async throws -> Void
    /// How many releases live only in the cloud and would become unplayable if
    /// this device disconnected. `0` means nothing is at risk; the UI renders the
    /// sentence itself, with its own locale's plural rules.
    public let cloudOnlyReleaseCount: @Sendable () async throws -> UInt64
    /// Retry failed cloud-outbox uploads now (clears their backoff and kicks
    /// the sync loop).
    public let retryOutbox: @Sendable () async throws -> Void
    /// Rename a library by id. If the id matches the active library the
    /// reactive `ConfigState` handles the rename; for any other local
    /// library the on-disk `config.yaml` is edited in place. The sidebar
    /// can rename either the active row or any inactive local row.
    public let renameLibrary:
        @Sendable (_ libraryId: String, _ newName: String) throws -> Void
    /// Cancel whatever transition a release is mid-flight — pin, upload, or
    /// unmanage — leaving it in its prior state. A no-op if nothing's running.
    public let cancelReleaseTransition:
        @Sendable (_ releaseId: String) async throws
            -> Void
    /// Pause or resume the cloud-upload pipeline. In-flight uploads finish; the
    /// queue stops draining until resumed.
    public let setSyncPaused: @Sendable (_ paused: Bool) async throws -> Void
    // periphery:ignore - called from the iOS pull-to-refresh; the macOS target
    // periphery analyzes doesn't use it (sync is automatic there).
    /// Wake the sync loop to run a cycle now (manual pull-to-refresh).
    /// Non-throwing, and a no-op when no provider is connected — a failure that
    /// left no connection needs `reconnectSync`, which builds one.
    public let triggerSync: @Sendable () -> Void
    /// Retry sync with the provider this library already has configured:
    /// connects when a failed launch left no connection, then runs a cycle now.
    /// Distinct from `triggerSync`, which only wakes an already-running loop and
    /// is silently a no-op when the startup connect never landed. A failure is
    /// recorded as the sync-status error the failure banner reads, so the banner
    /// shows why the retry didn't take; it also throws so the caller can end its
    /// in-progress state.
    public let reconnectSync: @Sendable () async throws -> Void
    /// Hand one operation from the sync status's blocked list back to the sync
    /// loop, which revalidates it and runs a cycle. Takes the operation's `id`.
    /// The operation leaves the list on the next status the UI receives; one
    /// whose cause still stands simply blocks again, and it throws when the id
    /// names nothing blocked so the caller can end its in-progress state.
    public let retryBlockedSyncOperation:
        @Sendable (_ id: String) async throws -> Void
    /// Delete the active library's encryption key from the OS keyring.
    /// The current session keeps working (the key stays in memory);
    /// the next launch lands on the unlock screen.
    public let lockActiveLibrary: @Sendable () async throws -> Void
    /// How many blob uploads the sync drain runs at once (1...8). A persisted
    /// device-local config write, unlike the runtime pause control: it throws on
    /// an out-of-range value or a failed write, so the picker can snap back.
    /// Takes effect the next time the library's coven handle opens.
    public let setMaxConcurrentUploads: @Sendable (_ n: UInt32) throws -> Void

    public init(
        disconnectCloudProvider: @escaping @Sendable () async throws -> Void = {
        },
        saveSyncConfig:
            @escaping @Sendable (BridgeSaveSyncConfig) async throws -> Void = {
                _ in
            },
        generateRestoreCode: @escaping @Sendable () async throws -> String = {
            ""
        },
        getMembers: @escaping @Sendable () async throws -> BridgeMembership = {
            throw StubError.notImplemented
        },
        startDevicePairing:
            @escaping @Sendable () async throws -> BridgeDevicePairingSession =
            {
                throw StubError.notImplemented
            },
        removeMember: @escaping @Sendable (String) async throws -> Void = {
            _ in
            throw StubError.notImplemented
        },
        cloudOnlyReleaseCount:
            @escaping @Sendable () async throws
            -> UInt64 = { throw StubError.notImplemented },
        retryOutbox: @escaping @Sendable () async throws -> Void = {},
        renameLibrary: @escaping @Sendable (String, String) throws -> Void = {
            _,
            _ in
            throw StubError.notImplemented
        },
        cancelReleaseTransition:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        setSyncPaused: @escaping @Sendable (Bool) async throws -> Void = { _ in
        },
        triggerSync: @escaping @Sendable () -> Void = {},
        reconnectSync: @escaping @Sendable () async throws -> Void = {},
        retryBlockedSyncOperation:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        lockActiveLibrary: @escaping @Sendable () async throws -> Void = {
            throw StubError.notImplemented
        },
        setMaxConcurrentUploads: @escaping @Sendable (UInt32) throws -> Void = {
            _ in
        }
    ) {
        self.disconnectCloudProvider = disconnectCloudProvider
        self.saveSyncConfig = saveSyncConfig
        self.generateRestoreCode = generateRestoreCode
        self.getMembers = getMembers
        self.startDevicePairing = startDevicePairing
        self.removeMember = removeMember
        self.cloudOnlyReleaseCount = cloudOnlyReleaseCount
        self.retryOutbox = retryOutbox
        self.triggerSync = triggerSync
        self.reconnectSync = reconnectSync
        self.retryBlockedSyncOperation = retryBlockedSyncOperation
        self.renameLibrary = renameLibrary
        self.cancelReleaseTransition = cancelReleaseTransition
        self.setSyncPaused = setSyncPaused
        self.lockActiveLibrary = lockActiveLibrary
        self.setMaxConcurrentUploads = setMaxConcurrentUploads
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            disconnectCloudProvider: {
                try await handle.disconnectCloudProvider()
            },
            saveSyncConfig: { try await handle.saveSyncConfig(configData: $0) },
            generateRestoreCode: { try await handle.generateRestoreCode() },
            getMembers: { try await handle.getMembers() },
            startDevicePairing: { try await handle.startDevicePairing() },
            removeMember: { try await handle.removeMember(publicKeyHex: $0) },
            cloudOnlyReleaseCount: {
                try await handle.cloudOnlyReleaseCount()
            },
            retryOutbox: { try await handle.retryOutbox() },
            renameLibrary: {
                try handle.renameLibrary(libraryId: $0, name: $1)
            },
            cancelReleaseTransition: {
                try await handle.cancelReleaseTransition(releaseId: $0)
            },
            setSyncPaused: { try await handle.setSyncPaused(paused: $0) },
            triggerSync: { handle.triggerSync() },
            reconnectSync: { try await handle.reconnectSync() },
            retryBlockedSyncOperation: {
                try await handle.retryBlockedSyncOperation(id: $0)
            },
            lockActiveLibrary: { try await handle.lockActiveLibrary() },
            setMaxConcurrentUploads: {
                try handle.setMaxConcurrentUploads(n: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Sync { Sync() }
    #endif

    /// Generate a fresh restore code and persist it to iCloud Keychain
    /// against `libraryId`. The bridge owns the asynchronous generation
    /// runtime; only the synchronous Keychain write moves to a detached
    /// worker. Errors surface through `onError` on the main actor. Called on
    /// startup (via `BaeApp.openLibrary` when `isSyncReady`) and from
    /// `LibrarySettingsTab` after every successful sync-config change.
    ///
    /// The two steps report separately because they fail for unrelated reasons
    /// and only one of them names the keychain — a user told "couldn't create a
    /// restore code" when the keychain refused the write has been pointed at the
    /// wrong thing.
    public func storeRestoreCodeInKeychain(
        libraryId: String,
        onError: @escaping @Sendable (DisplayError) -> Void
    ) {
        Task { [generateRestoreCode] in
            let code: String
            do {
                code = try await generateRestoreCode()
            }
            catch {
                await report(
                    error,
                    context: String(
                        localized: "Couldn't create a restore code"
                    ),
                    to: onError
                )
                return
            }
            do {
                try await DetachedWork.run {
                    try KeychainService.saveRestoreCode(
                        libraryId: libraryId,
                        code: code
                    )
                }
            }
            catch {
                await report(
                    error,
                    context: String(
                        localized:
                            "Couldn't save the restore code to your keychain"
                    ),
                    to: onError
                )
            }
        }
    }

    /// Hand a failure to `onError` on the main actor under the line naming what
    /// was being attempted. A failure core says has no line to show — a
    /// cancellation — is not reported at all.
    private func report(
        _ error: any Error,
        context: String,
        to onError: @escaping @Sendable (DisplayError) -> Void
    ) async {
        guard let displayed = DisplayError(error) else { return }
        await MainActor.run {
            onError(displayed.addingContext(context))
        }
    }
}
