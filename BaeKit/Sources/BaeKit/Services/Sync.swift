import Foundation

public final class Sync: Sendable, Observable {
    /// Connect an S3 bucket as this library's storage, registering this
    /// device under its name, each step written into the setup's stream.
    public let saveSyncConfig:
        @Sendable (BridgeSaveSyncConfig, BridgeLibrarySetup) async throws ->
            BridgeStorageSetup
    /// A new storage setup to follow, for the view that shows the setup.
    public let followStorageSetup:
        @MainActor @Sendable () -> StorageSetupFollowing
    /// Cancel the running storage setup. Before its point of no return it
    /// is undone and its call throws `BridgeError.Cancelled`; past it, it
    /// finishes, and Cancel no longer applies.
    public let cancelStorageSetup: @Sendable () -> BridgeSetupCancellation
    /// Dismiss the storage setup the sync status shows ended undone, by its
    /// operation.
    public let dismissUndoneStorageSetup: @Sendable (Int64) async throws -> Void
    public let generateRestoreCode:
        @Sendable () async throws -> BridgeRestoreCodeShare
    /// Add Device's code, with the watch for the person's device that joins
    /// from it.
    public let addDevice: @Sendable () async throws -> BridgeAddDevice
    public let removeMember: @Sendable (String) async throws -> Void
    public let removeDevice:
        @Sendable (UInt64) async throws -> BridgeProviderSignOut?
    /// The person did the provider step for a device they removed: nothing
    /// it sent can land any more.
    public let confirmDeviceCutOff:
        @Sendable (UInt64) async throws -> BridgeDeviceCutOff
    /// Whether this device's person's account holds the library's storage:
    /// only then can they confirm a device cut off.
    public let holdsStore: @Sendable () async throws -> Bool
    public let setMemberRole:
        @Sendable (String, BridgeMemberRole) async throws -> Void
    /// Dismiss a finished admin change by its operation.
    public let acknowledgeAdminChange: @Sendable (Int64) async throws -> Void
    /// Invite a person, with a role, the access they reach storage with, and
    /// how long the invite lasts; answers the new invite's id, which the open
    /// invites list from its write on. The person types their own name when
    /// they join.
    public let invitePerson:
        @Sendable (
            BridgeMemberRole, BridgeInviteAccess, BridgeInviteLifetime
        ) async throws -> String
    public let approveJoinRequest:
        @Sendable (BridgeJoinRequest) async throws -> Void
    public let declineJoinRequest:
        @Sendable (BridgeJoinRequest) async throws -> Void
    public let cancelInvite: @Sendable (String) async throws -> Void
    public let setHistorySharing:
        @Sendable (BridgeHistorySharing) async throws -> Void
    public let createProfile: @Sendable (String) async throws -> Void
    /// Rename this device's person's profile, which everyone in the library
    /// sees.
    public let renameProfile: @Sendable (String) async throws -> Void
    public let retryOutbox: @Sendable () async throws -> Void
    public let renameLibrary: @Sendable (String) async throws -> Void
    public let cancelReleaseTransition: @Sendable (String) async throws -> Void
    public let setSyncPaused: @Sendable (Bool) async throws -> Void
    public let reconnectSync: @Sendable () async throws -> Void
    public let setMaxConcurrentUploads: @Sendable (UInt32) async throws -> Void
    public let storageLocation:
        @Sendable () async throws -> BridgeStorageLocation?
    /// The requests that ask sync to run now, none of which waits for it.
    public let nudges: SyncNudges
    /// What a pending record's action asks core to do.
    public let pendingActions: SyncPendingActions

    public init(
        saveSyncConfig:
            @escaping @Sendable (BridgeSaveSyncConfig, BridgeLibrarySetup)
            async throws -> BridgeStorageSetup = { _, _ in
                throw StubError.notImplemented
            },
        followStorageSetup:
            @escaping @MainActor @Sendable () -> StorageSetupFollowing = {
                preconditionFailure("this Sync sets no storage up")
            },
        cancelStorageSetup:
            @escaping @Sendable () -> BridgeSetupCancellation = {
                .noSetup
            },
        dismissUndoneStorageSetup:
            @escaping @Sendable (Int64) async throws -> Void = { _ in
                throw StubError.notImplemented
            },
        generateRestoreCode:
            @escaping @Sendable () async throws -> BridgeRestoreCodeShare = {
                throw StubError.notImplemented
            },
        addDevice: @escaping @Sendable () async throws -> BridgeAddDevice = {
            throw StubError.notImplemented
        },
        removeMember: @escaping @Sendable (String) async throws -> Void = { _ in
            throw StubError.notImplemented
        },
        removeDevice:
            @escaping @Sendable (UInt64) async throws -> BridgeProviderSignOut? =
            { _ in throw StubError.notImplemented },
        confirmDeviceCutOff:
            @escaping @Sendable (UInt64) async throws -> BridgeDeviceCutOff =
            { _ in throw StubError.notImplemented },
        holdsStore: @escaping @Sendable () async throws -> Bool = {
            throw StubError.notImplemented
        },
        setMemberRole:
            @escaping @Sendable (String, BridgeMemberRole) async throws -> Void =
            { _, _ in throw StubError.notImplemented },
        acknowledgeAdminChange:
            @escaping @Sendable (Int64) async throws -> Void =
            { _ in throw StubError.notImplemented },
        invitePerson:
            @escaping @Sendable (
                BridgeMemberRole, BridgeInviteAccess, BridgeInviteLifetime
            ) async throws -> String = { _, _, _ in
                throw StubError.notImplemented
            },
        approveJoinRequest:
            @escaping @Sendable (BridgeJoinRequest) async throws -> Void = {
                _ in throw StubError.notImplemented
            },
        declineJoinRequest:
            @escaping @Sendable (BridgeJoinRequest) async throws -> Void = {
                _ in throw StubError.notImplemented
            },
        cancelInvite: @escaping @Sendable (String) async throws -> Void = { _ in
            throw StubError.notImplemented
        },
        setHistorySharing:
            @escaping @Sendable (BridgeHistorySharing) async throws -> Void = {
                _ in
                throw StubError.notImplemented
            },
        createProfile: @escaping @Sendable (String) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        renameProfile: @escaping @Sendable (String) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        retryOutbox: @escaping @Sendable () async throws -> Void = {
            throw StubError.notImplemented
        },
        renameLibrary: @escaping @Sendable (String) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        cancelReleaseTransition:
            @escaping @Sendable (String) async throws -> Void = { _ in
                throw StubError.notImplemented
            },
        setSyncPaused: @escaping @Sendable (Bool) async throws -> Void = { _ in
            throw StubError.notImplemented
        },
        reconnectSync: @escaping @Sendable () async throws -> Void = {
            throw StubError.notImplemented
        },
        setMaxConcurrentUploads:
            @escaping @Sendable (UInt32) async throws -> Void = { _ in
                throw StubError.notImplemented
            },
        storageLocation:
            @escaping @Sendable () async throws -> BridgeStorageLocation? = {
                throw StubError.notImplemented
            },
        nudges: SyncNudges = SyncNudges(),
        pendingActions: SyncPendingActions = SyncPendingActions()
    ) {
        self.saveSyncConfig = saveSyncConfig
        self.followStorageSetup = followStorageSetup
        self.cancelStorageSetup = cancelStorageSetup
        self.dismissUndoneStorageSetup = dismissUndoneStorageSetup
        self.generateRestoreCode = generateRestoreCode
        self.addDevice = addDevice
        self.removeMember = removeMember
        self.removeDevice = removeDevice
        self.confirmDeviceCutOff = confirmDeviceCutOff
        self.holdsStore = holdsStore
        self.setMemberRole = setMemberRole
        self.acknowledgeAdminChange = acknowledgeAdminChange
        self.invitePerson = invitePerson
        self.approveJoinRequest = approveJoinRequest
        self.declineJoinRequest = declineJoinRequest
        self.cancelInvite = cancelInvite
        self.setHistorySharing = setHistorySharing
        self.createProfile = createProfile
        self.renameProfile = renameProfile
        self.retryOutbox = retryOutbox
        self.renameLibrary = renameLibrary
        self.cancelReleaseTransition = cancelReleaseTransition
        self.setSyncPaused = setSyncPaused
        self.reconnectSync = reconnectSync
        self.setMaxConcurrentUploads = setMaxConcurrentUploads
        self.storageLocation = storageLocation
        self.nudges = nudges
        self.pendingActions = pendingActions
    }

    /// The production wiring over the open library's handle; a storage
    /// setup's steps are pulled on `host`'s runtime. It has one closure for
    /// each of the handle's calls, so it grows with them.
    public convenience init(
        handle: any AppHandleProtocol,
        deviceName: String,
        host: BridgeHost
    ) {
        self.init(
            saveSyncConfig: {
                try await handle.saveSyncConfig(
                    configData: $0,
                    deviceName: deviceName,
                    setup: $1
                )
            },
            followStorageSetup: { StorageSetupFollowing(host: host) },
            cancelStorageSetup: { handle.cancelStorageSetup() },
            dismissUndoneStorageSetup: {
                try await handle.dismissUndoneStorageSetup(operation: $0)
            },
            generateRestoreCode: { try await handle.generateRestoreCode() },
            addDevice: { try await handle.addDevice() },
            removeMember: { try await handle.removeMember(member: $0) },
            removeDevice: { try await handle.removeDevice(device: $0) },
            confirmDeviceCutOff: {
                try await handle.confirmDeviceCutOff(device: $0)
            },
            holdsStore: { try await handle.holdsStore() },
            setMemberRole: {
                try await handle.setMemberRole(member: $0, role: $1)
            },
            acknowledgeAdminChange: {
                try await handle.acknowledgeAdminChange(operation: $0)
            },
            invitePerson: {
                try await handle.invitePerson(
                    role: $0,
                    access: $1,
                    lifetime: $2
                )
            },
            approveJoinRequest: {
                try await handle.approveJoinRequest(request: $0)
            },
            declineJoinRequest: {
                try await handle.declineJoinRequest(request: $0)
            },
            cancelInvite: { try await handle.cancelInvite(invite: $0) },
            setHistorySharing: {
                try await handle.setHistorySharing(sharing: $0)
            },
            createProfile: { try await handle.createProfile(name: $0) },
            renameProfile: { try await handle.renameProfile(name: $0) },
            retryOutbox: { try await handle.retryOutbox() },
            renameLibrary: { try await handle.renameLibrary(name: $0) },
            cancelReleaseTransition: {
                try await handle.cancelReleaseTransition(releaseId: $0)
            },
            setSyncPaused: { try await handle.setSyncPaused(paused: $0) },
            reconnectSync: { try await handle.reconnectSync() },
            setMaxConcurrentUploads: {
                try await handle.setMaxConcurrentUploads(n: $0)
            },
            storageLocation: { try await handle.storageLocation() },
            nudges: SyncNudges(handle: handle),
            pendingActions: SyncPendingActions(handle: handle)
        )
    }

    #if DEBUG
        public static func stub() -> Sync { Sync() }
    #endif
}

/// The requests that ask sync to run now. Only the person's retry waits for
/// the pass it asks for; the sync status reports what the others did.
public struct SyncNudges: Sendable {
    /// Ask for a pass, leaving every backoff in place: bae's own triggers.
    public let triggerSync: @Sendable () -> Void
    /// The person asked to retry everything now: ends every ordinary backoff
    /// and syncs. Pulling to refresh calls it, and returns once the first
    /// pass after it ends, with what came of it.
    public let retryNow: @Sendable () async throws -> BridgeSyncRetryOutcome

    public init(
        triggerSync: @escaping @Sendable () -> Void = {},
        retryNow:
            @escaping @Sendable () async throws -> BridgeSyncRetryOutcome = {
                .synced
            }
    ) {
        self.triggerSync = triggerSync
        self.retryNow = retryNow
    }

    public init(handle: any AppHandleProtocol) {
        self.init(
            triggerSync: { handle.triggerSync() },
            retryNow: { try await handle.retryNow() }
        )
    }
}

/// What a pending record's action asks core to do, by the action.
public struct SyncPendingActions: Sendable {
    /// Retry one of coven's failed operations.
    public let retryOperation: @Sendable (Int64) async throws -> Void
    /// Pin a release again, as a failed pin's record offers.
    public let retryDownload: @Sendable (String) async throws -> Void
    /// End a release's failed move to the cloud, as its record offers: the
    /// release stays on this device only, as it was.
    public let cancelFailedMove: @Sendable (Int64) async throws -> Void

    public init(
        retryOperation: @escaping @Sendable (Int64) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        retryDownload: @escaping @Sendable (String) async throws -> Void = {
            _ in throw StubError.notImplemented
        },
        cancelFailedMove: @escaping @Sendable (Int64) async throws -> Void = {
            _ in throw StubError.notImplemented
        }
    ) {
        self.retryOperation = retryOperation
        self.retryDownload = retryDownload
        self.cancelFailedMove = cancelFailedMove
    }

    public init(handle: any AppHandleProtocol) {
        self.init(
            retryOperation: { try await handle.retryPendingOperation(id: $0) },
            retryDownload: {
                try await handle.queuePinReleases(releaseIds: [$0])
            },
            cancelFailedMove: {
                try await handle.cancelFailedReleaseMove(id: $0)
            }
        )
    }
}
