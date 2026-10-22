import BaeKit
import Foundation

/// Release-level mutations and the remote-cover fetch the editor needs.
/// Covers cover changes, pinning, deletion, primary-release selection,
/// opening a release edit session (whose own commands apply a catalog, pick
/// its album and save), and the remote-cover gallery fetch.
@MainActor
final class ReleaseEditor: Observable {
    let changeCover:
        @Sendable (
            _ releaseId: String, _ selection: BridgeCoverSelection
        ) async throws -> Void
    private let moveReleasesToCloudAction:
        @Sendable (_ releaseIds: [String]) async throws ->
            BridgeUploadReleasesOutcome
    private let outboxStore: OutboxStore
    let makeReleaseLocal:
        @Sendable (_ releaseId: String, _ newPath: String) async throws -> Void
    let deleteRelease: @Sendable (_ releaseId: String) async throws -> Void
    let setPrimaryRelease:
        @Sendable (_ albumId: String, _ releaseId: String) async throws -> Void
    /// Open an edit session of the caller's own on a release: its form as
    /// the library holds it, and the session's commands.
    let openReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> OpenedReleaseEdit
    /// The release's cover as it stands now, leaving any session as it is.
    let releaseCover:
        @Sendable (_ releaseId: String) async throws -> BridgeImageRef?
    let fetchRemoteCovers:
        @Sendable (_ target: BridgeCoverTarget) async throws ->
            BridgeRemoteCoverGallery

    init(
        changeCover:
            @escaping @Sendable (String, BridgeCoverSelection)
            async throws -> Void = { _, _ in },
        moveReleasesToCloud:
            @escaping @Sendable ([String]) async throws ->
            BridgeUploadReleasesOutcome =
            { _ in throw StubError.notImplemented },
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
        openReleaseEdit:
            @escaping @Sendable (String) async throws -> OpenedReleaseEdit =
            { _ in throw StubError.notImplemented },
        releaseCover:
            @escaping @Sendable (String) async throws -> BridgeImageRef? = {
                _ in nil
            },
        fetchRemoteCovers:
            @escaping @Sendable (BridgeCoverTarget) async throws ->
            BridgeRemoteCoverGallery = {
                _ in throw StubError.notImplemented
            }
    ) {
        self.changeCover = changeCover
        moveReleasesToCloudAction = moveReleasesToCloud
        self.outboxStore = outboxStore
        self.makeReleaseLocal = makeReleaseLocal
        self.deleteRelease = deleteRelease
        self.setPrimaryRelease = setPrimaryRelease
        self.openReleaseEdit = openReleaseEdit
        self.releaseCover = releaseCover
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
                try await handle.uploadReleases(releaseIds: $0)
            },
            outboxStore: outboxStore,
            makeReleaseLocal: {
                try await handle.detachReleaseAudio(releaseId: $0, newPath: $1)
            },
            deleteRelease: { try await handle.deleteRelease(releaseId: $0) },
            setPrimaryRelease: {
                try await handle.setPrimaryRelease(albumId: $0, releaseId: $1)
            },
            openReleaseEdit: {
                let session = try await handle.openReleaseEdit(releaseId: $0)
                return OpenedReleaseEdit(
                    seed: session.opened(),
                    session: ReleaseEditSessionCommands(
                        apply: { try await session.applyCatalog(link: $0) },
                        setAlbum: { try await session.setAlbum(albumId: $0) },
                        save: { try await session.save(edit: $0) }
                    )
                )
            },
            releaseCover: { try await handle.releaseCover(releaseId: $0) },
            fetchRemoteCovers: {
                try await handle.fetchRemoteCovers(target: $0)
            }
        )
    }

    /// The commands an edit session on a release sends core, finding albums
    /// to pick with `searchAlbums`.
    func editCommands(
        searchAlbums:
            @escaping @MainActor (String) async throws ->
            [BridgeAlbumChoice]
    ) -> ReleaseEditCommands {
        ReleaseEditCommands(
            open: openReleaseEdit,
            searchAlbums: searchAlbums,
            cover: releaseCover
        )
    }

    /// Move a Local release to Cloud without a standing Local frame between
    /// the foreground command and the retained outbox subscription. The bridge
    /// returns the exact revision it published; `OutboxStore` owns the handoff
    /// until that revision arrives or proves the upload already finished.
    /// Core keeps the releases here as the stored import storage choice says.
    func moveReleasesToCloud(_ releaseIds: [String]) async throws {
        let command = outboxStore.beginCloudUploads(
            forReleases: releaseIds
        )
        let outcome: BridgeUploadReleasesOutcome
        do {
            outcome = try await moveReleasesToCloudAction(releaseIds)
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

    func moveReleaseToCloud(_ releaseId: String) async throws {
        try await moveReleasesToCloud([releaseId])
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
