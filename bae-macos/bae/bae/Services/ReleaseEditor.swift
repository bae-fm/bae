import BaeKit
import Foundation

/// Release-level mutations and the remote-cover fetch the editor needs.
/// Covers cover changes, pinning, deletion, primary-release selection, the
/// release edit session (open, apply a catalog, pick its album, save,
/// discard), and the
/// remote-cover gallery fetch.
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
    /// Open an edit session on a release: its form as the library holds it.
    let openReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> BridgeReleaseEditSeed
    /// Apply a catalog release to the session; the form comes back filled
    /// with its every value, held until the save.
    let applyReleaseEditCatalog:
        @Sendable (_ releaseId: String, _ link: BridgePressingLink)
            async throws -> BridgeReleaseEditSeed
    /// Pick the album the session files the release under (`nil`: the one
    /// its records name); returns the album it files it under now.
    let setReleaseEditAlbum:
        @Sendable (_ releaseId: String, _ albumId: String?) async throws ->
            BridgeDraftAlbum
    /// Save the session with the form; returns the album the release is on
    /// after it.
    let saveReleaseEdit:
        @Sendable (_ releaseId: String, _ edit: BridgeRawReleaseEdit)
            async throws -> String
    /// Drop the session: nothing is written.
    let discardReleaseEdit: @Sendable (_ releaseId: String) async throws -> Void
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
            @escaping @Sendable (String) async throws -> BridgeReleaseEditSeed =
            { _ in throw StubError.notImplemented },
        applyReleaseEditCatalog:
            @escaping @Sendable (String, BridgePressingLink) async throws ->
            BridgeReleaseEditSeed = { _, _ in throw StubError.notImplemented },
        setReleaseEditAlbum:
            @escaping @Sendable (String, String?) async throws ->
            BridgeDraftAlbum = { _, _ in .new },
        saveReleaseEdit:
            @escaping @Sendable (String, BridgeRawReleaseEdit)
            async throws -> String = { _, _ in "" },
        discardReleaseEdit:
            @escaping @Sendable (String) async throws -> Void = { _ in },
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
        self.applyReleaseEditCatalog = applyReleaseEditCatalog
        self.setReleaseEditAlbum = setReleaseEditAlbum
        self.saveReleaseEdit = saveReleaseEdit
        self.discardReleaseEdit = discardReleaseEdit
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
                try await handle.openReleaseEdit(releaseId: $0)
            },
            applyReleaseEditCatalog: {
                try await handle.applyReleaseEditCatalog(
                    releaseId: $0,
                    link: $1
                )
            },
            setReleaseEditAlbum: {
                try await handle.setReleaseEditAlbum(
                    releaseId: $0,
                    albumId: $1
                )
            },
            saveReleaseEdit: {
                try await handle.saveReleaseEdit(releaseId: $0, edit: $1)
            },
            discardReleaseEdit: {
                try await handle.discardReleaseEdit(releaseId: $0)
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
            apply: applyReleaseEditCatalog,
            setAlbum: setReleaseEditAlbum,
            searchAlbums: searchAlbums,
            save: saveReleaseEdit,
            discard: discardReleaseEdit,
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
