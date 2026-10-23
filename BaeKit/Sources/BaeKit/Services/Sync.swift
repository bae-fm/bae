import Foundation

public final class Sync: Sendable, Observable {
    public let disconnectCloudProvider: @Sendable () async throws -> Void
    /// Connect an S3 bucket as this library's cloud home, registering this
    /// device under its name.
    public let saveSyncConfig:
        @Sendable (BridgeSaveSyncConfig) async throws -> BridgeStorageSetup
    public let generateRestoreCode:
        @Sendable () async throws -> BridgeRestoreCodeShare
    public let removeMember: @Sendable (String) async throws -> Void
    public let removeDevice:
        @Sendable (UInt64) async throws -> BridgeProviderSignOut?
    public let setMemberRole:
        @Sendable (String, BridgeMemberRole) async throws -> Void
    public let invitePerson:
        @Sendable (BridgeMemberRole, BridgeInviteAccess) async throws ->
            BridgeInvite
    public let approveJoinRequest:
        @Sendable (BridgeJoinRequest) async throws -> Void
    public let declineJoinRequest:
        @Sendable (BridgeJoinRequest) async throws -> Void
    public let cancelInvite: @Sendable (String) async throws -> Void
    public let setHistorySharing: @Sendable (BridgeSharing) async throws -> Void
    public let createProfile:
        @Sendable (String) async throws -> BridgePersonProfile
    public let cloudOnlyReleaseCount: @Sendable () async throws -> UInt64
    public let retryOutbox: @Sendable () async throws -> Void
    public let renameLibrary: @Sendable (String) async throws -> Void
    public let cancelReleaseTransition: @Sendable (String) async throws -> Void
    public let setSyncPaused: @Sendable (Bool) async throws -> Void
    public let reconnectSync: @Sendable () async throws -> Void
    public let retryPendingOperation: @Sendable (Int64) async throws -> Void
    public let setMaxConcurrentUploads: @Sendable (UInt32) async throws -> Void
    public let storageLocation:
        @Sendable () async throws -> BridgeStorageLocation?
    public let detectS3Settings:
        @Sendable (BridgeStorageLocation, BridgeS3Credentials) async throws ->
            BridgeS3Detection
    /// The requests that ask sync to run now, none of which waits for it.
    public let nudges: SyncNudges

    public init(
        disconnectCloudProvider: @escaping @Sendable () async throws -> Void = {
            throw StubError.notImplemented
        },
        saveSyncConfig:
            @escaping @Sendable (BridgeSaveSyncConfig) async throws ->
            BridgeStorageSetup = { _ in throw StubError.notImplemented },
        generateRestoreCode:
            @escaping @Sendable () async throws -> BridgeRestoreCodeShare = {
                throw StubError.notImplemented
            },
        removeMember: @escaping @Sendable (String) async throws -> Void = { _ in
            throw StubError.notImplemented
        },
        removeDevice:
            @escaping @Sendable (UInt64) async throws -> BridgeProviderSignOut? =
            { _ in throw StubError.notImplemented },
        setMemberRole:
            @escaping @Sendable (String, BridgeMemberRole) async throws -> Void =
            { _, _ in throw StubError.notImplemented },
        invitePerson:
            @escaping @Sendable (BridgeMemberRole, BridgeInviteAccess)
            async throws -> BridgeInvite = { _, _ in
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
            @escaping @Sendable (BridgeSharing) async throws -> Void = { _ in
                throw StubError.notImplemented
            },
        createProfile:
            @escaping @Sendable (String) async throws -> BridgePersonProfile = {
                _ in throw StubError.notImplemented
            },
        cloudOnlyReleaseCount: @escaping @Sendable () async throws -> UInt64 = {
            throw StubError.notImplemented
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
        retryPendingOperation:
            @escaping @Sendable (Int64) async throws -> Void = { _ in
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
        detectS3Settings:
            @escaping @Sendable (BridgeStorageLocation, BridgeS3Credentials)
            async throws -> BridgeS3Detection = { _, _ in
                throw StubError.notImplemented
            },
        nudges: SyncNudges = SyncNudges()
    ) {
        self.disconnectCloudProvider = disconnectCloudProvider
        self.saveSyncConfig = saveSyncConfig
        self.generateRestoreCode = generateRestoreCode
        self.removeMember = removeMember
        self.removeDevice = removeDevice
        self.setMemberRole = setMemberRole
        self.invitePerson = invitePerson
        self.approveJoinRequest = approveJoinRequest
        self.declineJoinRequest = declineJoinRequest
        self.cancelInvite = cancelInvite
        self.setHistorySharing = setHistorySharing
        self.createProfile = createProfile
        self.cloudOnlyReleaseCount = cloudOnlyReleaseCount
        self.retryOutbox = retryOutbox
        self.renameLibrary = renameLibrary
        self.cancelReleaseTransition = cancelReleaseTransition
        self.setSyncPaused = setSyncPaused
        self.reconnectSync = reconnectSync
        self.retryPendingOperation = retryPendingOperation
        self.setMaxConcurrentUploads = setMaxConcurrentUploads
        self.storageLocation = storageLocation
        self.detectS3Settings = detectS3Settings
        self.nudges = nudges
    }

    public convenience init(
        handle: any AppHandleProtocol,
        deviceName: String
    ) {
        self.init(
            disconnectCloudProvider: {
                try await handle.disconnectCloudProvider()
            },
            saveSyncConfig: {
                try await handle.saveSyncConfig(
                    configData: $0,
                    deviceName: deviceName
                )
            },
            generateRestoreCode: { try await handle.generateRestoreCode() },
            removeMember: { try await handle.removeMember(member: $0) },
            removeDevice: { try await handle.removeDevice(device: $0) },
            setMemberRole: {
                try await handle.setMemberRole(member: $0, role: $1)
            },
            invitePerson: {
                try await handle.invitePerson(role: $0, access: $1)
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
            cloudOnlyReleaseCount: { try await handle.cloudOnlyReleaseCount() },
            retryOutbox: { try await handle.retryOutbox() },
            renameLibrary: { try await handle.renameLibrary(name: $0) },
            cancelReleaseTransition: {
                try await handle.cancelReleaseTransition(releaseId: $0)
            },
            setSyncPaused: { try await handle.setSyncPaused(paused: $0) },
            reconnectSync: { try await handle.reconnectSync() },
            retryPendingOperation: {
                try await handle.retryPendingOperation(id: $0)
            },
            setMaxConcurrentUploads: {
                try await handle.setMaxConcurrentUploads(n: $0)
            },
            storageLocation: { try await handle.storageLocation() },
            detectS3Settings: {
                try await handle.detectS3Settings(location: $0, credentials: $1)
            },
            nudges: SyncNudges(handle: handle)
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
    /// Tell sync the device's network may be back; see
    /// `AppService.reportNetworkChange`.
    public let reportNetworkChange: @Sendable () -> Void

    public init(
        triggerSync: @escaping @Sendable () -> Void = {},
        retryNow:
            @escaping @Sendable () async throws -> BridgeSyncRetryOutcome = {
                .synced
            },
        reportNetworkChange: @escaping @Sendable () -> Void = {}
    ) {
        self.triggerSync = triggerSync
        self.retryNow = retryNow
        self.reportNetworkChange = reportNetworkChange
    }

    public init(handle: any AppHandleProtocol) {
        self.init(
            triggerSync: { handle.triggerSync() },
            retryNow: { try await handle.retryNow() },
            reportNetworkChange: { handle.reportNetworkChange() }
        )
    }
}
