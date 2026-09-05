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
    private let moveReleasesToCloudAction:
        @Sendable (_ releaseIds: [String], _ pin: Bool) async throws ->
            BridgeMakeReleasesRemoteOutcome
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
        @Sendable (_ releaseId: String) async throws -> BridgeReleaseEditSeed
    let updateReleaseMetadataUserEdit:
        @Sendable (_ releaseId: String, _ edit: BridgeReleaseUserEdit)
            async throws -> Void
    let resetReleaseEditToSource:
        @Sendable (_ releaseId: String) async throws -> BridgeRawReleaseEdit
    let fetchRemoteCovers:
        @Sendable (_ target: BridgeCoverTarget) async throws ->
            [BridgeRemoteCover]

    init(
        changeCover:
            @escaping @Sendable (String, BridgeCoverSelection)
            async throws -> Void = { _, _ in },
        moveReleasesToCloud:
            @escaping @Sendable ([String], Bool) async throws ->
            BridgeMakeReleasesRemoteOutcome =
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
            @escaping @Sendable (String) async throws -> BridgeReleaseEditSeed =
            { _ in throw StubError.notImplemented },
        updateReleaseMetadataUserEdit:
            @escaping @Sendable (String, BridgeReleaseUserEdit) async throws ->
            Void = { _, _ in },
        resetReleaseEditToSource:
            @escaping @Sendable (String) async throws -> BridgeRawReleaseEdit =
            { _ in throw StubError.notImplemented },
        fetchRemoteCovers:
            @escaping @Sendable (BridgeCoverTarget) async throws ->
            [BridgeRemoteCover] = {
                _ in []
            }
    ) {
        self.changeCover = changeCover
        moveReleasesToCloudAction = moveReleasesToCloud
        self.outboxStore = outboxStore
        self.makeReleaseLocal = makeReleaseLocal
        self.deleteRelease = deleteRelease
        self.setPrimaryRelease = setPrimaryRelease
        self.reIdentifyRelease = reIdentifyRelease
        self.seedReleaseEdit = seedReleaseEdit
        self.updateReleaseMetadataUserEdit = updateReleaseMetadataUserEdit
        self.resetReleaseEditToSource = resetReleaseEditToSource
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
            moveReleasesToCloud: {
                try await handle.makeReleasesRemote(releaseIds: $0, pin: $1)
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
                    reseed: $1
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
            resetReleaseEditToSource: {
                try await handle.resetReleaseEditToSource(releaseId: $0)
            },
            fetchRemoteCovers: {
                try await handle.fetchRemoteCovers(target: $0)
            }
        )
    }

    /// Move a Local release to Cloud without a standing Local frame between
    /// the foreground command and the retained outbox subscription. The bridge
    /// returns the exact revision it published; `OutboxStore` owns the handoff
    /// until that revision arrives or proves the upload already finished.
    func moveReleasesToCloud(_ releaseIds: [String], _ pin: Bool) async throws {
        let command = outboxStore.beginCloudUploads(
            forReleases: releaseIds
        )
        let outcome: BridgeMakeReleasesRemoteOutcome
        do {
            outcome = try await moveReleasesToCloudAction(releaseIds, pin)
        }
        catch {
            outboxStore.finishCloudUploads(for: command, receipt: nil)
            throw error
        }
        switch outcome {
        case .complete(let receipt):
            outboxStore.finishCloudUploads(for: command, receipt: receipt)
        case .partial(let receipt, let failure):
            outboxStore.finishCloudUploads(for: command, receipt: receipt)
            throw failure.error
        }
    }

    func moveReleaseToCloud(_ releaseId: String, _ pin: Bool) async throws {
        try await moveReleasesToCloud([releaseId], pin)
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
