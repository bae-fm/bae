import BaeKit
import Foundation

/// Release-level mutations and the remote-cover / edit-seed fetches
/// the editor needs to populate forms. Covers cover changes, pinning,
/// deletion, primary-release selection, re-identify, metadata
/// seeding/save/reset, and the remote-cover gallery fetch.
@MainActor
final class ReleaseEditor: Observable {
    let changeCover:
        @Sendable (
            _ releaseId: String, _ selection: BridgeCoverSelection
        ) async throws -> Void
    private let moveReleaseToCloudAction:
        @Sendable (_ releaseId: String, _ pin: Bool) async throws -> UInt64
    private let outboxStore: OutboxStore
    let makeReleaseLocal:
        @Sendable (_ releaseId: String, _ newPath: String) async throws -> Void
    let deleteRelease: @Sendable (_ releaseId: String) async throws -> Void
    let setPrimaryRelease:
        @Sendable (_ albumId: String, _ releaseId: String) async throws -> Void
    let reIdentifyRelease:
        @Sendable (_ releaseId: String, _ reseed: BridgeReleaseReseed)
            async throws -> String
    let seedReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> BridgeRawReleaseEdit
    let updateReleaseMetadataUserEdit:
        @Sendable (_ releaseId: String, _ edit: BridgeReleaseUserEdit)
            async throws -> Void
    let resetMetadataToSource:
        @Sendable (_ releaseId: String) async throws -> BridgeReleaseUserEdit
    let fetchRemoteCovers:
        @Sendable (_ releaseId: String) async throws -> [BridgeRemoteCover]

    init(
        changeCover:
            @escaping @Sendable (String, BridgeCoverSelection)
            async throws -> Void = { _, _ in },
        moveReleaseToCloud:
            @escaping @Sendable (String, Bool) async throws -> UInt64 =
            { _, _ in throw StubError.notImplemented },
        outboxStore: OutboxStore,
        makeReleaseLocal:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        deleteRelease: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        setPrimaryRelease:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        reIdentifyRelease:
            @escaping @Sendable (String, BridgeReleaseReseed) async throws ->
            String = { _, _ in "" },
        seedReleaseEdit:
            @escaping @Sendable (String) async throws -> BridgeRawReleaseEdit =
            { _ in throw StubError.notImplemented },
        updateReleaseMetadataUserEdit:
            @escaping @Sendable (String, BridgeReleaseUserEdit) async throws ->
            Void = { _, _ in },
        resetMetadataToSource:
            @escaping @Sendable (String) async throws -> BridgeReleaseUserEdit =
            { _ in throw StubError.notImplemented },
        fetchRemoteCovers:
            @escaping @Sendable (String) async throws -> [BridgeRemoteCover] = {
                _ in []
            }
    ) {
        self.changeCover = changeCover
        moveReleaseToCloudAction = moveReleaseToCloud
        self.outboxStore = outboxStore
        self.makeReleaseLocal = makeReleaseLocal
        self.deleteRelease = deleteRelease
        self.setPrimaryRelease = setPrimaryRelease
        self.reIdentifyRelease = reIdentifyRelease
        self.seedReleaseEdit = seedReleaseEdit
        self.updateReleaseMetadataUserEdit = updateReleaseMetadataUserEdit
        self.resetMetadataToSource = resetMetadataToSource
        self.fetchRemoteCovers = fetchRemoteCovers
    }

    convenience init(
        handle: any AppHandleProtocol,
        outboxStore: OutboxStore
    ) {
        self.init(
            changeCover: {
                try await handle.changeCover(
                    releaseId: $0,
                    selection: $1
                )
            },
            moveReleaseToCloud: {
                try await handle.makeReleaseRemote(releaseId: $0, pin: $1)
            },
            outboxStore: outboxStore,
            makeReleaseLocal: {
                try await handle.makeReleaseLocal(releaseId: $0, newPath: $1)
            },
            deleteRelease: { try await handle.deleteRelease(releaseId: $0) },
            setPrimaryRelease: {
                try await handle.setPrimaryRelease(albumId: $0, releaseId: $1)
            },
            reIdentifyRelease: {
                try await handle.reIdentifyRelease(
                    releaseId: $0,
                    identityChoice: $1
                )
            },
            seedReleaseEdit: {
                try await handle.seedReleaseEdit(releaseId: $0)
            },
            updateReleaseMetadataUserEdit: {
                try await handle.updateReleaseMetadataUserEdit(
                    releaseId: $0,
                    edit: $1
                )
            },
            resetMetadataToSource: {
                try await handle.resetMetadataToSource(releaseId: $0)
            },
            fetchRemoteCovers: {
                try await handle.fetchRemoteCovers(releaseId: $0)
            }
        )
    }

    /// Move a Local release to Cloud without a standing Local frame between
    /// the foreground command and the retained outbox subscription. The bridge
    /// returns the exact revision it published; `OutboxStore` owns the handoff
    /// until that revision arrives or proves the upload already finished.
    func moveReleaseToCloud(_ releaseId: String, _ pin: Bool) async throws {
        outboxStore.beginCloudUpload(forRelease: releaseId)
        do {
            let revision = try await moveReleaseToCloudAction(releaseId, pin)
            outboxStore.cloudUploadQueued(
                forRelease: releaseId,
                atRevision: revision
            )
        }
        catch {
            outboxStore.cloudUploadFailed(forRelease: releaseId)
            throw error
        }
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> ReleaseEditor {
            ReleaseEditor(
                outboxStore: OutboxStore(
                    snapshot: OutboxStore.emptySnapshot
                )
            )
        }
    #endif
}
