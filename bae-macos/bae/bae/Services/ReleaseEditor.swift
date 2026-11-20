import BaeKit
import Foundation

/// Release-level mutations and the cover lookups the editor needs.
/// Covers cover changes, pinning, deleting a release or a whole album,
/// primary-release selection, opening a release editor's form (whose
/// commands change it, apply a catalog, pick its album and save it),
/// reading one of a release's documents, and the lookups of the covers the
/// catalogs offer a release's or a candidate's picker.
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
    let moveReleasesToThisDevice:
        @Sendable (_ releaseIds: [String], _ folder: URL) async throws -> Void
    let deleteRelease: @Sendable (_ releaseId: String) async throws -> Void
    /// Delete an album: every release it holds and what each owns.
    let deleteAlbum: @Sendable (_ albumId: String) async throws -> Void
    let setPrimaryRelease:
        @Sendable (_ albumId: String, _ releaseId: String) async throws -> Void
    /// Open a form of the caller's own on a release: the release's values
    /// as the library holds them, the draft read that follows the form, and
    /// the form's commands.
    let openReleaseEdit:
        @Sendable (_ releaseId: String) async throws -> OpenedReleaseEdit
    /// The text of the release's document `fileId`, decoded by core.
    let readReleaseDocument:
        @Sendable (_ releaseId: String, _ fileId: String) async throws ->
            String
    /// Look up what the catalogs the candidate links to offer its cover
    /// picker, which core stores for the open picker's live read.
    let refreshCandidateCovers:
        @Sendable (_ candidateKey: Data) async throws -> Void
    /// Look up what the catalogs offer the release's cover picker, which core
    /// stores for the open picker's live read.
    let refreshReleaseCovers:
        @Sendable (_ releaseId: String) async throws -> Void
    /// A new apply of a catalog release and its steps, for the editor to
    /// name what it waits on; `nil` where no bridge is behind the editor.
    let catalogApply: @Sendable () -> (any CatalogApplySource)?

    init(
        changeCover:
            @escaping @Sendable (String, BridgeCoverSelection)
            async throws -> Void = { _, _ in },
        moveReleasesToCloud:
            @escaping @Sendable ([String]) async throws ->
            BridgeUploadReleasesOutcome =
            { _ in throw StubError.notImplemented },
        moveReleasesToThisDevice:
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
        refreshCandidateCovers:
            @escaping @Sendable (Data) async throws -> Void = { _ in },
        refreshReleaseCovers:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        catalogApply: @escaping @Sendable () -> (any CatalogApplySource)? = {
            nil
        }
    ) {
        self.changeCover = changeCover
        moveReleasesToCloudAction = moveReleasesToCloud
        self.moveReleasesToThisDevice = moveReleasesToThisDevice
        self.deleteRelease = deleteRelease
        self.deleteAlbum = deleteAlbum
        self.setPrimaryRelease = setPrimaryRelease
        self.openReleaseEdit = openReleaseEdit
        self.readReleaseDocument = readReleaseDocument
        self.refreshCandidateCovers = refreshCandidateCovers
        self.refreshReleaseCovers = refreshReleaseCovers
        self.catalogApply = catalogApply
    }

    convenience init(handle: any AppHandleProtocol) {
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
            moveReleasesToThisDevice: {
                try await handle.moveReleasesToThisDevice(
                    releaseIds: $0,
                    folder: $1.pathBytes
                )
            },
            deleteRelease: { try await handle.deleteRelease(releaseId: $0) },
            deleteAlbum: { try await handle.deleteAlbum(albumId: $0) },
            setPrimaryRelease: {
                try await handle.setPrimaryRelease(albumId: $0, releaseId: $1)
            },
            openReleaseEdit: { try await Self.openEdit(handle, releaseId: $0) },
            readReleaseDocument: {
                try await handle.readReleaseDocument(releaseId: $0, fileId: $1)
            },
            refreshCandidateCovers: {
                try await handle.refreshCandidateCovers(candidateKey: $0)
            },
            refreshReleaseCovers: {
                try await handle.refreshReleaseCovers(releaseId: $0)
            },
            catalogApply: { handle.catalogApply() }
        )
    }

    /// A form of the caller's own on the release, read from `handle`, the
    /// draft read that follows it, and the commands it takes.
    private static func openEdit(
        _ handle: any AppHandleProtocol,
        releaseId: String
    ) async throws -> OpenedReleaseEdit {
        let form = HeldReleaseEditForm(
            form: try await handle.openReleaseEdit(releaseId: releaseId),
            drafts: handle.subscribeReleaseEditDraft(),
            handle: handle
        )
        try form.showDraft()
        return OpenedReleaseEdit(
            values: form.values,
            drafts: PulledValues(form.drafts),
            form: ReleaseEditFormCommands(
                setField: { field, value in
                    try await form.change {
                        $0.withField(field: field, value: value)
                    }
                },
                setLabels: { labels in
                    try await form.change { $0.withLabels(labels: labels) }
                },
                setBarcodes: { barcodes in
                    try await form.change {
                        $0.withBarcodes(barcodes: barcodes)
                    }
                },
                setPressingFact: { fact in
                    try await form.change { $0.withPressingFact(fact: fact) }
                },
                setAlbumArtists: { assignments in
                    try await form.change {
                        $0.withAlbumArtists(assignments: assignments)
                    }
                },
                setTrack: { track in
                    try await form.change { try $0.withTrack(track: track) }
                },
                apply: { try await form.apply(link: $0, apply: $1) },
                applyFileTags: { try await form.applyFileTags() },
                clear: { try await form.clear() },
                unlink: { _ = try await form.change { $0.unlinked() } },
                setAlbum: { pick in
                    _ = try await form.change { $0.withAlbum(pick: pick) }
                },
                setSheetSide: { sheetFileId, side in
                    _ = try await form.change {
                        $0.withSheetSide(sheetFileId: sheetFileId, side: side)
                    }
                },
                setSheetFile: { sheetFileId, fileReference, audioFileId in
                    _ = try await form.change {
                        $0.withSheetFile(
                            sheetFileId: sheetFileId,
                            fileReference: fileReference,
                            audioFileId: audioFileId
                        )
                    }
                },
                cancel: { try await form.reopen() },
                save: { try await form.save() }
            )
        )
    }

    /// The commands a release editor sends core beyond its form, finding
    /// albums to pick with `searchAlbums`.
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

/// The form one release editor holds, as core gave it, and the draft read
/// that follows it. Core keeps no form: each change replaces the form held
/// here and sets it on the read, and an apply, a save or a Cancel hands core
/// the form and holds the form core returns. Each command that can change
/// the form's values returns them, for the editor to draw.
@MainActor
final class HeldReleaseEditForm {
    private var form: BridgeReleaseEditForm
    let drafts: ReleaseEditDraftSubscription
    private let handle: any AppHandleProtocol

    init(
        form: BridgeReleaseEditForm,
        drafts: ReleaseEditDraftSubscription,
        handle: any AppHandleProtocol
    ) {
        self.form = form
        self.drafts = drafts
        self.handle = handle
    }

    /// The values of the form held now.
    var values: BridgeRawReleaseEdit {
        form.edit()
    }

    /// Point the draft read at the form held now.
    func showDraft() throws {
        try drafts.setForm(form: form)
    }

    /// Hold the form `next` makes of the one held now; returns its values.
    func change(
        _ next:
            @Sendable (BridgeReleaseEditForm) throws -> BridgeReleaseEditForm
    ) throws -> BridgeRawReleaseEdit {
        form = try next(form)
        try showDraft()
        return values
    }

    /// Hold the form with the catalog release `link` picks applied, each
    /// network read the apply waits on written to `apply`'s row; returns its
    /// values.
    func apply(link: BridgePressingLink, apply: BridgeCatalogApply?)
        async throws -> BridgeRawReleaseEdit
    {
        try await hold { [handle] form in
            try await handle.applyReleaseEditCatalog(
                form: form,
                link: link,
                apply: apply
            )
        }
    }

    /// Hold the form with what the release's own files say in place of its
    /// values; returns them.
    func applyFileTags() async throws -> BridgeRawReleaseEdit {
        try await hold { [handle] in
            try await handle.applyReleaseEditFileTags(form: $0)
        }
    }

    /// Hold the form with its metadata cleared; returns its values.
    func clear() async throws -> BridgeRawReleaseEdit {
        try await hold { [handle] in try await handle.clearReleaseEdit(form: $0)
        }
    }

    /// Save the form held now, then hold the form opened on what the save
    /// wrote; returns the album the release is on after the save and the
    /// values of that form.
    func save() async throws -> SavedReleaseEdit {
        let saved = try await handle.saveReleaseEdit(form: form)
        form = saved.form
        try showDraft()
        return SavedReleaseEdit(albumId: saved.albumId, values: values)
    }

    /// Hold the form opened again on the release as the library holds it
    /// now, which drops every change; returns its values.
    func reopen() async throws -> BridgeRawReleaseEdit {
        try await hold { [handle] in
            try await handle.openReleaseEdit(releaseId: $0.releaseId())
        }
    }

    /// Hold the form core returns for the one held now, and point the draft
    /// read at it; returns its values.
    private func hold(
        _ next: (BridgeReleaseEditForm) async throws -> BridgeReleaseEditForm
    ) async throws -> BridgeRawReleaseEdit {
        form = try await next(form)
        try showDraft()
        return values
    }
}
