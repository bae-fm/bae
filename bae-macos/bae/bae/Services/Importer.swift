import BaeKit
import Foundation

private struct ImportOperations: Sendable {
    let candidateSourceFolders: @Sendable (Data) async throws -> [Data]
    let combineFolder:
        @Sendable (BridgeFolderReleaseDecisionKey) async throws -> Data
    let separateCandidate: @Sendable (Data) async throws -> Void
    let addWatchedFolder: @Sendable (Data) async throws -> Void
    let removeWatchedFolder: @Sendable (Data) async throws -> Void
    let refreshWatchedFolder: @Sendable (Data) async throws -> Void
    let setSheetBinding:
        @Sendable (Data, String, String, String?) async throws -> Void
    let applyCandidateExternalMetadata:
        @Sendable (Data, BridgePressingLink, BridgeCatalogApply?) async throws
            -> BridgePaneOutcome
    /// `nil` where no bridge is behind the importer: a preview or a test.
    let catalogApply: @Sendable () -> CatalogApplySteps?
    let applyCandidateFileMetadata:
        @Sendable (Data) async throws -> BridgePaneOutcome
    let keepCandidateDraft: @Sendable (Data) async throws -> BridgePaneOutcome
    let linkCandidateSharedAlbum:
        @Sendable (Data) async throws -> BridgePaneOutcome
    let unlinkCandidateRelease:
        @Sendable (Data) async throws -> BridgePaneOutcome
    let clearCandidateMetadata: @Sendable (Data) async throws -> UInt64
    let setSheetDisc:
        @Sendable (Data, String, BridgeSheetDisc) async throws -> Void
    let autoIdentifyRelease: @Sendable (String) async throws -> Void
    let editReleaseLookupChoices:
        @Sendable (String, BridgeLookupChoiceEdit) async throws -> Void
    let endReleaseIdentification: @Sendable (String) async throws -> Void
    let startCandidateSearch:
        @Sendable (Data, BridgeSearchQuery) async throws -> Void
    let retryCandidateSearch: @Sendable (Data) async throws -> Void
    /// `nil` where no bridge is behind the importer: a preview or a test.
    let subscribeCandidateRuntime:
        @Sendable (Data) -> CandidateRuntimeSubscription?
    /// `nil` where no bridge is behind the importer: a preview or a test
    /// that reads no library membership.
    let subscribeLibraryStatuses: @Sendable () -> LibraryStatusSubscription?
    let editCandidateLookupChoices:
        @Sendable (Data, BridgeLookupChoiceEdit) async throws ->
            BridgePaneOutcome
    let rerunIdentifyForCandidate: @Sendable (Data) -> Void
    let cancelAllIdentification: @Sendable () async throws -> Void
    let cancelCandidateIdentification: @Sendable (Data) async throws -> Void
    let cancelAllImports: @Sendable () -> Void
    let moveCandidatePane: @Sendable (Data, BridgePaneMove) async throws -> Void
    let identifyAutomatically: @Sendable (Data) async throws -> Void
    let setCandidateSearchForm:
        @Sendable (Data, BridgeSearchForm) async throws -> Void
    let setCandidateCover:
        @Sendable (Data, BridgeCoverSelection) async throws -> Void
    let setCandidateEditField:
        @Sendable (Data, BridgeCandidateEditField, String) async throws ->
            Void
    let setCandidateLabels:
        @Sendable (Data, [BridgeRawLabelEdit]) async throws -> Void
    let setCandidateBarcodes: @Sendable (Data, [String]) async throws -> Void
    let setCandidateAlbum: @Sendable (Data, String?) async throws -> Void
    let readCandidateDocument: @Sendable (Data, String) async throws -> String
    let searchAlbums: @Sendable (String) async throws -> [BridgeAlbumChoice]
    let setCandidatePressingFact:
        @Sendable (Data, BridgePressingFactEdit) async throws -> Void
    let setCandidateAlbumArtists:
        @Sendable (Data, [BridgeCredited]) async throws -> Void
    let setCandidateTrackEdit:
        @Sendable (Data, BridgeRawTrackEdit) async throws -> Void
    let startImport: @Sendable (Data) async throws -> BridgePaneOutcome
    let setIdentifyAutomatically:
        @MainActor @Sendable (Bool) async throws -> Void
    let setMetadataSourceEnabled:
        @MainActor @Sendable (BridgeLookupCatalog, Bool) async throws -> Void
    let setImportWhenIdentified:
        @MainActor @Sendable (Bool) async throws -> Void
    let setImportDestination:
        @MainActor @Sendable (BridgeImportDestination) async throws -> Void
}

extension ImportOperations {
    // Flat forwarding from AppHandleProtocol into immutable operation values.
    // swiftlint:disable:next function_body_length
    static func live(handle: any AppHandleProtocol, host: BridgeHost)
        -> ImportOperations
    {
        ImportOperations(
            candidateSourceFolders: {
                try await handle.candidateSourceFolders(key: $0)
            },
            combineFolder: {
                try await handle.combineFolder(key: $0)
            },
            separateCandidate: {
                try await handle.separateCandidate(key: $0)
            },
            addWatchedFolder: {
                try await handle.addWatchedFolder(path: $0)
            },
            removeWatchedFolder: {
                try await handle.removeWatchedFolder(path: $0)
            },
            refreshWatchedFolder: {
                try await handle.refreshWatchedFolder(path: $0)
            },
            setSheetBinding: {
                try await handle.setSheetBinding(
                    candidateKey: $0,
                    sheetFileId: $1,
                    fileReference: $2,
                    audioFileId: $3
                )
            },
            applyCandidateExternalMetadata: {
                try await handle.selectCandidateRelease(
                    candidateKey: $0,
                    link: $1,
                    apply: $2
                )
            },
            catalogApply: { CatalogApplySteps(host: host) },
            applyCandidateFileMetadata: {
                try await handle.readCandidateFileTags(candidateKey: $0)
            },
            keepCandidateDraft: {
                try await handle.keepCandidateDraft(candidateKey: $0)
            },
            linkCandidateSharedAlbum: {
                try await handle.linkCandidateSharedAlbum(candidateKey: $0)
            },
            unlinkCandidateRelease: {
                try await handle.unlinkCandidateRelease(candidateKey: $0)
            },
            clearCandidateMetadata: {
                try await handle.clearCandidateMetadata(candidateKey: $0)
            },
            setSheetDisc: {
                try await handle.setSheetDisc(
                    candidateKey: $0,
                    sheetFileId: $1,
                    disc: $2
                )
            },
            autoIdentifyRelease: {
                try await handle.autoIdentifyRelease(releaseId: $0)
            },
            editReleaseLookupChoices: {
                try await handle.editReleaseLookupChoices(
                    releaseId: $0,
                    edit: $1
                )
            },
            endReleaseIdentification: {
                try await handle.endReleaseIdentification(releaseId: $0)
            },
            startCandidateSearch: {
                try await handle.startCandidateSearch(
                    candidateKey: $0,
                    query: $1
                )
            },
            retryCandidateSearch: {
                try await handle.retryCandidateSearch(candidateKey: $0)
            },
            subscribeCandidateRuntime: {
                handle.subscribeCandidateRuntime(candidateKey: $0)
            },
            subscribeLibraryStatuses: { handle.subscribeLibraryStatuses() },
            editCandidateLookupChoices: {
                try await handle.editCandidateLookupChoices(
                    candidateKey: $0,
                    edit: $1
                )
            },
            rerunIdentifyForCandidate: {
                handle.rerunIdentifyForCandidate(candidateKey: $0)
            },
            cancelAllIdentification: {
                try await handle.cancelAllIdentification()
            },
            cancelCandidateIdentification: {
                try await handle.cancelCandidateIdentification(candidateKey: $0)
            },
            cancelAllImports: {
                handle.cancelAllImports()
            },
            moveCandidatePane: {
                try await handle.moveCandidatePane(
                    candidateKey: $0,
                    paneMove: $1
                )
            },
            identifyAutomatically: {
                try await handle.identifyAutomatically(candidateKey: $0)
            },
            setCandidateSearchForm: {
                try await handle.setCandidateSearchForm(
                    candidateKey: $0,
                    search: $1
                )
            },
            setCandidateCover: {
                try await handle.setCandidateCover(candidateKey: $0, cover: $1)
            },
            setCandidateEditField: {
                try await handle.setCandidateEditField(
                    candidateKey: $0,
                    field: $1,
                    value: $2
                )
            },
            setCandidateLabels: {
                try await handle.setCandidateLabels(
                    candidateKey: $0,
                    labels: $1
                )
            },
            setCandidateBarcodes: {
                try await handle.setCandidateBarcodes(
                    candidateKey: $0,
                    barcodes: $1
                )
            },
            setCandidateAlbum: {
                try await handle.setCandidateAlbum(
                    candidateKey: $0,
                    albumId: $1
                )
            },
            readCandidateDocument: {
                try await handle.readCandidateDocument(
                    candidateKey: $0,
                    fileId: $1
                )
            },
            searchAlbums: {
                try await handle.searchAlbums(query: $0)
            },
            setCandidatePressingFact: {
                try await handle.setCandidatePressingFact(
                    candidateKey: $0,
                    fact: $1
                )
            },
            setCandidateAlbumArtists: {
                try await handle.setCandidateAlbumArtists(
                    candidateKey: $0,
                    assignments: $1
                )
            },
            setCandidateTrackEdit: {
                try await handle.setCandidateTrackEdit(
                    candidateKey: $0,
                    track: $1
                )
            },
            startImport: {
                try await handle.startImport(candidateKey: $0)
            },
            setIdentifyAutomatically: {
                try await handle.setIdentifyAutomatically(enabled: $0)
            },
            setMetadataSourceEnabled: {
                try await handle.setMetadataSourceEnabled(
                    source: $0,
                    enabled: $1
                )
            },
            setImportWhenIdentified: {
                try await handle.setImportWhenIdentified(enabled: $0)
            },
            setImportDestination: {
                try await handle.setImportDestination(destination: $0)
            }
        )
    }
}

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, and commit.
final class Importer: Sendable, Observable {
    private let operations: ImportOperations

    init(
        candidateSourceFolders:
            @escaping @Sendable (Data) async throws -> [Data] = { _ in
                throw StubError.notImplemented
            },
        combineFolder:
            @escaping @Sendable (BridgeFolderReleaseDecisionKey) async throws
            -> Data = { _ in throw StubError.notImplemented },
        separateCandidate: @escaping @Sendable (Data) async throws -> Void =
            { _ in throw StubError.notImplemented },
        addWatchedFolder: @escaping @Sendable (Data) async throws -> Void = {
            _ in
        },
        removeWatchedFolder: @escaping @Sendable (Data) async throws -> Void =
            {
                _ in
            },
        refreshWatchedFolder:
            @escaping @Sendable (Data) async throws -> Void = { _ in },
        setSheetBinding:
            @escaping @Sendable (Data, String, String, String?) async throws
            -> Void =
            { _, _, _, _ in },
        applyCandidateExternalMetadata:
            @escaping @Sendable (Data, BridgePressingLink, BridgeCatalogApply?)
            async throws -> BridgePaneOutcome = { _, _, _ in
                throw StubError.notImplemented
            },
        catalogApply: @escaping @Sendable () -> CatalogApplySteps? = { nil },
        applyCandidateFileMetadata:
            @escaping @Sendable (Data) async throws -> BridgePaneOutcome = {
                _ in throw StubError.notImplemented
            },
        keepCandidateDraft:
            @escaping @Sendable (Data) async throws -> BridgePaneOutcome = {
                _ in throw StubError.notImplemented
            },
        linkCandidateSharedAlbum:
            @escaping @Sendable (Data) async throws -> BridgePaneOutcome = {
                _ in throw StubError.notImplemented
            },
        unlinkCandidateRelease:
            @escaping @Sendable (Data) async throws -> BridgePaneOutcome = {
                _ in throw StubError.notImplemented
            },
        clearCandidateMetadata:
            @escaping @Sendable (Data) async throws -> UInt64 = { _ in
                throw StubError.notImplemented
            },
        setSheetDisc:
            @escaping @Sendable (Data, String, BridgeSheetDisc) async throws
            -> Void = { _, _, _ in },
        autoIdentifyRelease:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        editReleaseLookupChoices:
            @escaping @Sendable (String, BridgeLookupChoiceEdit) async throws
            -> Void = { _, _ in },
        endReleaseIdentification:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        startCandidateSearch:
            @escaping @Sendable (Data, BridgeSearchQuery) async throws ->
            Void = { _, _ in },
        retryCandidateSearch:
            @escaping @Sendable (Data) async throws -> Void = { _ in },
        subscribeCandidateRuntime:
            @escaping @Sendable (Data) -> CandidateRuntimeSubscription? = {
                _ in nil
            },
        subscribeLibraryStatuses:
            @escaping @Sendable () -> LibraryStatusSubscription? = { nil },
        editCandidateLookupChoices:
            @escaping @Sendable (Data, BridgeLookupChoiceEdit) async throws ->
            BridgePaneOutcome = { _, _ in .done },
        rerunIdentifyForCandidate:
            @escaping @Sendable (Data) -> Void = { _ in },
        cancelAllIdentification:
            @escaping @Sendable () async throws -> Void = {},
        cancelCandidateIdentification:
            @escaping @Sendable (Data) async throws -> Void = { _ in },
        cancelAllImports: @escaping @Sendable () -> Void = {},
        moveCandidatePane:
            @escaping @Sendable (Data, BridgePaneMove) async throws -> Void =
            {
                _,
                _ in
            },
        identifyAutomatically:
            @escaping @Sendable (Data) async throws -> Void = { _ in },
        setCandidateSearchForm:
            @escaping @Sendable (Data, BridgeSearchForm) async throws -> Void =
            {
                _,
                _ in
            },
        setCandidateCover:
            @escaping @Sendable (Data, BridgeCoverSelection) async throws ->
            Void = { _, _ in },
        setCandidateEditField:
            @escaping @Sendable (Data, BridgeCandidateEditField, String)
            async throws -> Void = { _, _, _ in },
        setCandidateLabels:
            @escaping @Sendable (Data, [BridgeRawLabelEdit]) async throws
            -> Void = { _, _ in },
        setCandidateBarcodes:
            @escaping @Sendable (Data, [String]) async throws
            -> Void = { _, _ in },
        setCandidateAlbum:
            @escaping @Sendable (Data, String?) async throws
            -> Void = { _, _ in },
        readCandidateDocument:
            @escaping @Sendable (Data, String) async throws -> String = {
                _,
                _ in throw StubError.notImplemented
            },
        searchAlbums:
            @escaping @Sendable (String) async throws
            -> [BridgeAlbumChoice] = { _ in [] },
        setCandidatePressingFact:
            @escaping @Sendable (Data, BridgePressingFactEdit) async throws
            -> Void = { _, _ in },
        setCandidateAlbumArtists:
            @escaping @Sendable (Data, [BridgeCredited]) async throws
            -> Void = { _, _ in },
        setCandidateTrackEdit:
            @escaping @Sendable (Data, BridgeRawTrackEdit) async throws ->
            Void = { _, _ in },
        startImport:
            @escaping @Sendable (Data) async throws -> BridgePaneOutcome = {
                _ in .done
            },
        setIdentifyAutomatically:
            @escaping @MainActor @Sendable (Bool) async throws -> Void = { _ in
            },
        setMetadataSourceEnabled:
            @escaping @MainActor @Sendable (
                BridgeLookupCatalog, Bool
            ) async throws -> Void = { _, _ in },
        setImportWhenIdentified:
            @escaping @MainActor @Sendable (Bool) async throws -> Void = { _ in
            },
        setImportDestination:
            @escaping @MainActor @Sendable (BridgeImportDestination)
            async throws -> Void = { _ in }
    ) {
        operations = ImportOperations(
            candidateSourceFolders: candidateSourceFolders,
            combineFolder: combineFolder,
            separateCandidate: separateCandidate,
            addWatchedFolder: addWatchedFolder,
            removeWatchedFolder: removeWatchedFolder,
            refreshWatchedFolder: refreshWatchedFolder,
            setSheetBinding: setSheetBinding,
            applyCandidateExternalMetadata: applyCandidateExternalMetadata,
            catalogApply: catalogApply,
            applyCandidateFileMetadata: applyCandidateFileMetadata,
            keepCandidateDraft: keepCandidateDraft,
            linkCandidateSharedAlbum: linkCandidateSharedAlbum,
            unlinkCandidateRelease: unlinkCandidateRelease,
            clearCandidateMetadata: clearCandidateMetadata,
            setSheetDisc: setSheetDisc,
            autoIdentifyRelease: autoIdentifyRelease,
            editReleaseLookupChoices: editReleaseLookupChoices,
            endReleaseIdentification: endReleaseIdentification,
            startCandidateSearch: startCandidateSearch,
            retryCandidateSearch: retryCandidateSearch,
            subscribeCandidateRuntime: subscribeCandidateRuntime,
            subscribeLibraryStatuses: subscribeLibraryStatuses,
            editCandidateLookupChoices: editCandidateLookupChoices,
            rerunIdentifyForCandidate: rerunIdentifyForCandidate,
            cancelAllIdentification: cancelAllIdentification,
            cancelCandidateIdentification: cancelCandidateIdentification,
            cancelAllImports: cancelAllImports,
            moveCandidatePane: moveCandidatePane,
            identifyAutomatically: identifyAutomatically,
            setCandidateSearchForm: setCandidateSearchForm,
            setCandidateCover: setCandidateCover,
            setCandidateEditField: setCandidateEditField,
            setCandidateLabels: setCandidateLabels,
            setCandidateBarcodes: setCandidateBarcodes,
            setCandidateAlbum: setCandidateAlbum,
            readCandidateDocument: readCandidateDocument,
            searchAlbums: searchAlbums,
            setCandidatePressingFact: setCandidatePressingFact,
            setCandidateAlbumArtists: setCandidateAlbumArtists,
            setCandidateTrackEdit: setCandidateTrackEdit,
            startImport: startImport,
            setIdentifyAutomatically: setIdentifyAutomatically,
            setMetadataSourceEnabled: setMetadataSourceEnabled,
            setImportWhenIdentified: setImportWhenIdentified,
            setImportDestination: setImportDestination
        )
    }

    private init(operations: ImportOperations) {
        self.operations = operations
    }
}

extension Importer {
    func candidateSourceFolders(_ key: Data) async throws -> [Data] {
        try await operations.candidateSourceFolders(key)
    }

    func combineFolder(
        _ key: BridgeFolderReleaseDecisionKey
    ) async throws -> Data {
        try await operations.combineFolder(key)
    }

    func separateCandidate(_ key: Data) async throws {
        try await operations.separateCandidate(key)
    }

    /// Take in the folder at `path`: watch it, or read again the watched
    /// folder that covers it.
    func addWatchedFolder(_ path: Data) async throws {
        try await operations.addWatchedFolder(path)
    }

    func removeWatchedFolder(_ path: Data) async throws {
        try await operations.removeWatchedFolder(path)
    }

    func refreshWatchedFolder(_ path: Data) async throws {
        try await operations.refreshWatchedFolder(path)
    }

    func setSheetBinding(
        _ candidateKey: Data,
        _ sheetFileId: String,
        _ fileReference: String,
        _ audioFileId: String?
    ) async throws {
        try await operations.setSheetBinding(
            candidateKey,
            sheetFileId,
            fileReference,
            audioFileId
        )
    }

    /// Link the candidate to the release a pick names, claiming every source
    /// that pick carried, and read its draft from it, each network read it
    /// waits on written to `apply`'s stream.
    func applyCandidateExternalMetadata(
        _ candidateKey: Data,
        link: BridgePressingLink,
        apply: BridgeCatalogApply?
    ) async throws -> BridgePaneOutcome {
        try await operations.applyCandidateExternalMetadata(
            candidateKey,
            link,
            apply
        )
    }

    /// A new apply of a catalog release and its steps, for the row that
    /// shows what it waits on; `nil` where no bridge is behind the importer.
    func catalogApply() -> CatalogApplySteps? {
        operations.catalogApply()
    }

    /// Read the candidate's files' own tags into its draft, a pane command:
    /// its failure is stated on the pane.
    func applyCandidateFileMetadata(_ candidateKey: Data) async throws
        -> BridgePaneOutcome
    {
        try await operations.applyCandidateFileMetadata(candidateKey)
    }

    /// Keep the candidate's own draft over what its lookup offered, a pane
    /// command that goes back to the draft; its failure is stated on the pane.
    func keepCandidateDraft(_ candidateKey: Data) async throws
        -> BridgePaneOutcome
    {
        try await operations.keepCandidateDraft(candidateKey)
    }

    /// Link the candidate to the album the pressings its lookup offers are
    /// of, its pressing unknown, taking what they agree on into its draft — a
    /// pane command that goes back to the draft; its failure is stated on the
    /// pane.
    func linkCandidateSharedAlbum(_ candidateKey: Data) async throws
        -> BridgePaneOutcome
    {
        try await operations.linkCandidateSharedAlbum(candidateKey)
    }

    /// Unlink the candidate from its release, a pane command whose failure is
    /// stated on the pane. The draft stays as it is.
    func unlinkCandidateRelease(_ candidateKey: Data) async throws
        -> BridgePaneOutcome
    {
        try await operations.unlinkCandidateRelease(candidateKey)
    }

    func clearCandidateMetadata(_ candidateKey: Data) async throws -> UInt64 {
        try await operations.clearCandidateMetadata(candidateKey)
    }

    func setSheetDisc(
        _ candidateKey: Data,
        _ sheetFileId: String,
        _ disc: BridgeSheetDisc
    ) async throws {
        try await operations.setSheetDisc(candidateKey, sheetFileId, disc)
    }

    /// Re-identify a library release, asking about what core holds for this
    /// session.
    func autoIdentifyRelease(_ releaseId: String) async throws {
        try await operations.autoIdentifyRelease(releaseId)
    }

    /// Make one change to what a library release's session asks about; core
    /// identifies it again from the changed choices.
    func editReleaseLookupChoices(
        _ releaseId: String,
        _ edit: BridgeLookupChoiceEdit
    ) async throws {
        try await operations.editReleaseLookupChoices(releaseId, edit)
    }

    /// End a library release's re-identify session: its identification stops
    /// and core forgets what it asked about.
    func endReleaseIdentification(_ releaseId: String) async throws {
        try await operations.endReleaseIdentification(releaseId)
    }

    /// Submit a candidate's typed search: every configured provider is asked
    /// at once and each answer lands on the candidate's runtime, which the
    /// pane already watches. Clears the failure the pane states.
    func startCandidateSearch(
        _ candidateKey: Data,
        _ query: BridgeSearchQuery
    ) async throws {
        try await operations.startCandidateSearch(candidateKey, query)
    }

    /// Re-ask only the providers whose part of the search failed.
    func retryCandidateSearch(_ candidateKey: Data) async throws {
        try await operations.retryCandidateSearch(candidateKey)
    }

    /// Open one live read of library membership for an import pane's
    /// offers; `nil` where no bridge is behind the importer.
    func subscribeLibraryStatuses() -> LibraryStatusSubscription? {
        operations.subscribeLibraryStatuses()
    }

    /// Make one change to what a candidate's identification asks about; core
    /// applies it to the choices it holds and starts the run that reads them.
    func editCandidateLookupChoices(
        _ candidateKey: Data,
        _ edit: BridgeLookupChoiceEdit
    ) async throws -> BridgePaneOutcome {
        try await operations.editCandidateLookupChoices(candidateKey, edit)
    }

    /// Identify a folder candidate again, over what the candidate says its
    /// lookup asks about and the sources the library asks now. Whatever is
    /// running for it is superseded, and any stored answer is replaced.
    ///
    /// Re-asking the providers that failed is this same command: every lookup
    /// goes out again, and core's response cache answers the ones that had
    /// already succeeded.
    func rerunIdentifyForCandidate(_ candidateKey: Data) {
        operations.rerunIdentifyForCandidate(candidateKey)
    }

    /// Take every candidate off the identification queue.
    func cancelAllIdentification() async throws {
        try await operations.cancelAllIdentification()
    }

    /// Stop identifying one candidate — take it off the identification
    /// queue, ending its run if one started — storing nothing, and put its
    /// pane back on the draft.
    func cancelCandidateIdentification(_ candidateKey: Data) async throws {
        try await operations.cancelCandidateIdentification(candidateKey)
    }

    /// Cancel every import that has not begun writing its release.
    func cancelAllImports() {
        operations.cancelAllImports()
    }

    /// Move the candidate's pane as the person asked; core moves it by the
    /// rule it moves every pane by.
    func moveCandidatePane(
        _ candidateKey: Data,
        _ paneMove: BridgePaneMove
    ) async throws {
        try await operations.moveCandidatePane(candidateKey, paneMove)
    }

    /// Show identification's results: the stored verdict as it stood when
    /// there is one, a run started when there is none.
    func identifyAutomatically(_ candidateKey: Data) async throws {
        try await operations.identifyAutomatically(candidateKey)
    }

    /// Record the typed-search form as the person left it.
    func setCandidateSearchForm(
        _ candidateKey: Data,
        _ search: BridgeSearchForm
    ) async throws {
        try await operations.setCandidateSearchForm(candidateKey, search)
    }

    /// Record the cover this candidate commits with.
    func setCandidateCover(
        _ candidateKey: Data,
        _ cover: BridgeCoverSelection
    ) async throws {
        try await operations.setCandidateCover(candidateKey, cover)
    }

    /// Record one album-level metadata field as the user left it.
    func setCandidateEditField(
        _ candidateKey: Data,
        _ field: BridgeCandidateEditField,
        _ value: String
    ) async throws {
        try await operations.setCandidateEditField(candidateKey, field, value)
    }

    /// Record this candidate's label rows as the user left them.
    func setCandidateLabels(
        _ candidateKey: Data,
        _ labels: [BridgeRawLabelEdit]
    ) async throws {
        try await operations.setCandidateLabels(candidateKey, labels)
    }

    /// Pick the library album this candidate joins, or (`nil`) leave it to
    /// the album its release names.
    func setCandidateAlbum(
        _ candidateKey: Data,
        _ albumId: String?
    ) async throws {
        try await operations.setCandidateAlbum(candidateKey, albumId)
    }

    /// The text of the candidate's document `fileId`, decoded by core.
    func readCandidateDocument(
        _ candidateKey: Data,
        _ fileId: String
    ) async throws -> String {
        try await operations.readCandidateDocument(candidateKey, fileId)
    }

    /// The library albums a query finds, for the album picker.
    func searchAlbums(_ query: String) async throws -> [BridgeAlbumChoice] {
        try await operations.searchAlbums(query)
    }

    /// Record this candidate's barcode rows as the person left them.
    func setCandidateBarcodes(
        _ candidateKey: Data,
        _ barcodes: [String]
    ) async throws {
        try await operations.setCandidateBarcodes(candidateKey, barcodes)
    }

    /// Record one choice of what this candidate's pressing is.
    func setCandidatePressingFact(
        _ candidateKey: Data,
        _ fact: BridgePressingFactEdit
    ) async throws {
        try await operations.setCandidatePressingFact(candidateKey, fact)
    }

    /// Replace the ordered album artists this candidate commits with.
    func setCandidateAlbumArtists(
        _ candidateKey: Data,
        _ assignments: [BridgeCredited]
    ) async throws {
        try await operations.setCandidateAlbumArtists(candidateKey, assignments)
    }

    /// Record one mapping-table row as the user left it.
    func setCandidateTrackEdit(
        _ candidateKey: Data,
        _ track: BridgeRawTrackEdit
    ) async throws {
        try await operations.setCandidateTrackEdit(candidateKey, track)
    }

    /// Commit a candidate from what core stores for it, a pane command.
    func startImport(_ candidateKey: Data) async throws -> BridgePaneOutcome {
        try await operations.startImport(candidateKey)
    }

    @MainActor
    func setIdentifyAutomatically(_ enabled: Bool) async throws {
        try await operations.setIdentifyAutomatically(enabled)
    }

    /// Ask, or stop asking, one metadata source — the same write behind the
    /// Find online header's checkboxes and the Settings ones. Throws when core
    /// refuses, which is when it would leave nothing to ask.
    @MainActor
    func setMetadataSourceEnabled(
        _ source: BridgeLookupCatalog,
        _ enabled: Bool
    ) async throws {
        try await operations.setMetadataSourceEnabled(source, enabled)
    }

    /// Import what an automatic run identifies as needing nothing, or stop.
    @MainActor
    func setImportWhenIdentified(_ enabled: Bool) async throws {
        try await operations.setImportWhenIdentified(enabled)
    }

    /// Where an import goes: this device or the cloud.
    @MainActor
    func setImportDestination(_ destination: BridgeImportDestination)
        async throws
    {
        try await operations.setImportDestination(destination)
    }

    convenience init(handle: any AppHandleProtocol, host: BridgeHost) {
        self.init(operations: .live(handle: handle, host: host))
    }

    /// What one candidate has in flight, for a screen to pull; `nil` where
    /// no bridge is behind the importer.
    func subscribeCandidateRuntime(_ key: Data) -> CandidateRuntimeSubscription?
    {
        operations.subscribeCandidateRuntime(key)
    }
}
