import BaeKit
import Foundation

/// Release-level mutations and the remote-cover fetch the editor needs.
/// Covers cover changes, pinning, deleting a release or a whole album,
/// primary-release selection, opening a release edit session (whose own
/// commands write each change, apply a catalog, pick its album and save),
/// reading one of a release's documents, and the remote-cover gallery fetch.
@MainActor
final class ReleaseEditor: Observable {
    let changeCover:
        @Sendable (
            _ releaseId: String, _ selection: BridgeCoverSelection
        ) async throws -> Void
    private let moveReleasesToCloudAction:
        @Sendable (_ releaseIds: [String]) async throws ->
            BridgeUploadReleasesOutcome
    /// Queue the releases to move to This Mac into a folder of their own each
    /// below `folder`.
    let makeReleasesLocal:
        @Sendable (_ releaseIds: [String], _ folder: URL) async throws -> Void
    let deleteRelease: @Sendable (_ releaseId: String) async throws -> Void
    /// Delete an album: every release it holds and what each owns.
    let deleteAlbum: @Sendable (_ albumId: String) async throws -> Void
    let setPrimaryRelease:
        @Sendable (_ albumId: String, _ releaseId: String) async throws -> Void
    /// Open an edit session of the caller's own on a release: its draft as
    /// the library holds the release, and the session's commands.
    let openReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> OpenedReleaseEdit
    /// The text of the release's document `fileId`, decoded by core.
    let readReleaseDocument:
        @Sendable (_ releaseId: String, _ fileId: String) async throws ->
            String
    let fetchRemoteCovers:
        @Sendable (_ target: BridgeCoverTarget) async throws ->
            BridgeRemoteCoverGallery
    /// A new apply of a catalog release and its steps, for the editor to
    /// name what it waits on; `nil` where no bridge is behind the editor.
    let catalogApply: @Sendable () -> CatalogApplySteps?

    init(
        changeCover:
            @escaping @Sendable (String, BridgeCoverSelection)
            async throws -> Void = { _, _ in },
        moveReleasesToCloud:
            @escaping @Sendable ([String]) async throws ->
            BridgeUploadReleasesOutcome =
            { _ in throw StubError.notImplemented },
        makeReleasesLocal:
            @escaping @Sendable ([String], URL) async throws -> Void = {
                _,
                _ in
            },
        deleteRelease: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        deleteAlbum: @escaping @Sendable (String) async throws -> Void = {
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
        readReleaseDocument:
            @escaping @Sendable (String, String) async throws -> String = {
                _,
                _ in throw StubError.notImplemented
            },
        fetchRemoteCovers:
            @escaping @Sendable (BridgeCoverTarget) async throws ->
            BridgeRemoteCoverGallery = {
                _ in throw StubError.notImplemented
            },
        catalogApply: @escaping @Sendable () -> CatalogApplySteps? = { nil }
    ) {
        self.changeCover = changeCover
        moveReleasesToCloudAction = moveReleasesToCloud
        self.makeReleasesLocal = makeReleasesLocal
        self.deleteRelease = deleteRelease
        self.deleteAlbum = deleteAlbum
        self.setPrimaryRelease = setPrimaryRelease
        self.openReleaseEdit = openReleaseEdit
        self.readReleaseDocument = readReleaseDocument
        self.fetchRemoteCovers = fetchRemoteCovers
        self.catalogApply = catalogApply
    }

    convenience init(handle: any AppHandleProtocol, host: BridgeHost) {
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
            makeReleasesLocal: {
                try await handle.makeReleasesLocal(
                    releaseIds: $0,
                    folder: $1.pathBytes
                )
            },
            deleteRelease: { try await handle.deleteRelease(releaseId: $0) },
            deleteAlbum: { try await handle.deleteAlbum(albumId: $0) },
            setPrimaryRelease: {
                try await handle.setPrimaryRelease(albumId: $0, releaseId: $1)
            },
            openReleaseEdit: {
                let session = try await handle.openReleaseEdit(releaseId: $0)
                return OpenedReleaseEdit(
                    draft: try await session.draft(),
                    session: ReleaseEditSessionCommands(
                        draft: { try await session.draft() },
                        setField: {
                            try await session.setField(field: $0, value: $1)
                        },
                        setLabels: { try await session.setLabels(labels: $0) },
                        setBarcodes: {
                            try await session.setBarcodes(barcodes: $0)
                        },
                        setPressingFact: {
                            try await session.setPressingFact(fact: $0)
                        },
                        setAlbumArtists: {
                            try await session.setAlbumArtists(assignments: $0)
                        },
                        setTrack: { try await session.setTrack(track: $0) },
                        apply: {
                            try await session.applyCatalog(link: $0, apply: $1)
                        },
                        setAlbum: { try await session.setAlbum(albumId: $0) },
                        save: { try await session.save() }
                    )
                )
            },
            readReleaseDocument: {
                try await handle.readReleaseDocument(releaseId: $0, fileId: $1)
            },
            fetchRemoteCovers: {
                try await handle.fetchRemoteCovers(target: $0)
            },
            catalogApply: { CatalogApplySteps(host: host) }
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
            catalogApply: catalogApply
        )
    }

    /// Move Local releases to Cloud. While core admits each, its row reads
    /// the transfer running; once admitted, its files waiting in the queue.
    func moveReleasesToCloud(_ releaseIds: [String]) async throws {
        switch try await moveReleasesToCloudAction(releaseIds) {
        case .complete:
            return
        case .partial(_, let failure):
            throw failure.error
        }
    }

    func moveReleaseToCloud(_ releaseId: String) async throws {
        try await moveReleasesToCloud([releaseId])
    }

    #if DEBUG
        // periphery:ignore
        static func stub() -> ReleaseEditor {
            ReleaseEditor()
        }
    #endif
}
