import BaeKit
import Foundation

/// What committing a candidate needs from the caller: where its files should
/// live. Everything about the release — the draft, provenance, edited fields,
/// the corrected rows, the cover — is stored under the candidate, so the
/// commit reads the very values the pane drew.
struct ImportCommitRequest: Sendable {
    let candidateKey: String
    let storageMode: BridgeStorageMode
    let pin: Bool
}

/// Whether the releases an import pane offers are already in the library,
/// read through one live query: which releases it checks is changed in place,
/// and each value answers every check at once.
struct LibraryStatusQuery: Sendable {
    /// Check these releases from now on; returns the revision the answer
    /// will carry.
    let setChecks: @Sendable ([BridgeLibraryCheck]) throws -> UInt64
    let next: @Sendable () async throws -> BridgeLibraryStatusSnapshot
    let cancel: @Sendable () async -> Void

    /// A query that checks nothing, for previews and tests that never read
    /// library membership.
    static let inert = LibraryStatusQuery(
        setChecks: { _ in 0 },
        next: { throw CancellationError() },
        cancel: {}
    )
}

private final class CandidateLiveStateSink: CandidateLiveStateCallback,
    @unchecked Sendable
{
    private let apply: @Sendable (BridgeCandidateLiveState) -> Void

    init(apply: @escaping @Sendable (BridgeCandidateLiveState) -> Void) {
        self.apply = apply
    }

    func onValue(value: BridgeCandidateLiveState) {
        apply(value)
    }
}

private struct ImportOperations: Sendable {
    let candidateSourceFolders: @Sendable (String) async throws -> [String]
    let combineCandidates: @Sendable ([String]) async throws -> String
    let combineFolder:
        @Sendable (BridgeFolderReleaseDecisionKey) async throws -> String
    let separateCandidate: @Sendable (String) async throws -> Void
    let chooseFolder: @Sendable (String) async throws -> BridgeChosenFolder
    let removeWatchedFolder: @Sendable (String) async throws -> Void
    let refreshWatchedFolder: @Sendable (String) async throws -> Void
    let setCandidateSkipped: @Sendable (String, Bool) async throws -> Void
    let setSheetBinding:
        @Sendable (String, String, String, String?) async throws -> Void
    let applyCandidateExternalMetadata:
        @Sendable (String, BridgeMetadataProvenance) async throws -> UInt64
    let applyCandidateFileMetadata: @Sendable (String) async throws -> UInt64
    let resetCandidateSetup: @Sendable (String) async throws -> Void
    let clearCandidateMetadata: @Sendable (String) async throws -> UInt64
    let setSheetDisc:
        @Sendable (String, String, BridgeSheetDisc) async throws -> Void
    let setFileRole:
        @Sendable (String, String, BridgeFileRoleChoice) async throws -> Void
    let autoIdentifyRelease:
        @Sendable (String, String, BridgeLookupChoices) -> Void
    let cancelAutoIdentify: @Sendable (String) -> Void
    let startCandidateSearch: @Sendable (String, BridgeSearchQuery) -> Void
    let retryCandidateSearch: @Sendable (String) -> Void
    let subscribeLibraryStatuses: @Sendable () -> LibraryStatusQuery
    let setCandidateLookupChoices:
        @Sendable (String, BridgeLookupChoices) async throws -> Void
    let rerunIdentifyForCandidate: @Sendable (String) -> Void
    let cancelIdentification: @Sendable ([String]) -> Void
    let cancelAllIdentification: @Sendable () -> Void
    let setCandidatePresentation:
        @Sendable (String, BridgeMetadataPresentation) async throws -> Void
    let setCandidateSearchForm:
        @Sendable (String, BridgeSearchForm) async throws -> Void
    let setCandidatePaneError: @Sendable (String, String?) async throws -> Void
    let setCandidateCover:
        @Sendable (String, BridgeCoverSelection) async throws -> Void
    let setCandidateEditField:
        @Sendable (String, BridgeCandidateEditField, String) async throws ->
            Void
    let setCandidatePressingFact:
        @Sendable (String, BridgePressingFactEdit) async throws -> Void
    let setCandidateAlbumArtists:
        @Sendable (String, [BridgeArtistAssignment]) async throws -> Void
    let setCandidateTrackEdit:
        @Sendable (String, BridgeRawTrackEdit) async throws -> Void
    let addCandidateTrack:
        @Sendable (String, BridgeAudioFile, BridgeCandidateAsRead) async throws
            -> Void
    let dropCandidateTrack: @Sendable (String, String) async throws -> Void
    let candidateRuntime: @Sendable (String) -> BridgeCandidateRuntimeSnapshot?
    let subscribeCandidateLiveState:
        @Sendable (
            String, BridgeCandidateActionBasis, CandidateLiveStateCallback
        ) -> any LiveSubscriptionProtocol
    let candidateSignals: @Sendable (String) -> Signals?
    let startImport: @Sendable (ImportCommitRequest) async throws -> Void
    let importReady: @Sendable (ImportCommitRequest) async throws -> Void
    let mergeCandidateArtistIdentityConflict:
        @Sendable (String, String) async throws -> Void
    let setIdentifyAutomatically:
        @MainActor @Sendable (Bool) async throws -> Void
    let setPrefillWithFileMetadata:
        @MainActor @Sendable (Bool) async throws -> Void
    let setMetadataSourceEnabled:
        @MainActor @Sendable (BridgeCatalog, Bool) async throws -> Void
}

extension ImportOperations {
    // Flat forwarding from AppHandleProtocol into immutable operation values.
    // swiftlint:disable:next function_body_length
    static func live(handle: any AppHandleProtocol) -> ImportOperations {
        ImportOperations(
            candidateSourceFolders: {
                try await handle.candidateSourceFolders(key: $0)
            },
            combineCandidates: {
                try await handle.combineCandidates(keys: $0)
            },
            combineFolder: {
                try await handle.combineFolder(key: $0)
            },
            separateCandidate: {
                try await handle.separateCandidate(key: $0)
            },
            chooseFolder: {
                try await handle.chooseImportFolder(path: $0)
            },
            removeWatchedFolder: {
                try await handle.removeWatchedFolder(path: $0)
            },
            refreshWatchedFolder: {
                try await handle.refreshWatchedFolder(path: $0)
            },
            setCandidateSkipped: {
                try await handle.setCandidateSkipped(path: $0, skipped: $1)
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
                try await handle.selectCandidateMetadataProvenance(
                    candidateKey: $0,
                    provenance: $1
                )
            },
            applyCandidateFileMetadata: {
                try await handle.selectCandidateMetadataProvenance(
                    candidateKey: $0,
                    provenance: .fileMetadata
                )
            },
            resetCandidateSetup: {
                try await handle.resetCandidateSetup(candidateKey: $0)
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
            setFileRole: {
                try await handle.setFileRole(
                    candidateKey: $0,
                    fileId: $1,
                    choice: $2
                )
            },
            autoIdentifyRelease: {
                handle.autoIdentifyRelease(
                    candidateKey: $0,
                    releaseId: $1,
                    choices: $2
                )
            },
            cancelAutoIdentify: {
                handle.cancelAutoIdentify(candidateKey: $0)
            },
            startCandidateSearch: {
                handle.startCandidateSearch(candidateKey: $0, query: $1)
            },
            retryCandidateSearch: {
                handle.retryCandidateSearch(candidateKey: $0)
            },
            subscribeLibraryStatuses: {
                let subscription = handle.subscribeLibraryStatuses()
                return LibraryStatusQuery(
                    setChecks: { try subscription.setChecks(checks: $0) },
                    next: { try await subscription.next() },
                    cancel: { try? await subscription.cancel() }
                )
            },
            setCandidateLookupChoices: {
                try await handle.setCandidateLookupChoices(
                    candidateKey: $0,
                    choices: $1
                )
            },
            rerunIdentifyForCandidate: {
                handle.rerunIdentifyForCandidate(candidateKey: $0)
            },
            cancelIdentification: {
                handle.cancelIdentification(candidateKeys: $0)
            },
            cancelAllIdentification: {
                handle.cancelAllIdentification()
            },
            setCandidatePresentation: {
                try await handle.setCandidatePresentation(
                    candidateKey: $0,
                    presentation: $1
                )
            },
            setCandidateSearchForm: {
                try await handle.setCandidateSearchForm(
                    candidateKey: $0,
                    search: $1
                )
            },
            setCandidatePaneError: {
                try await handle.setCandidatePaneError(
                    candidateKey: $0,
                    error: $1
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
            addCandidateTrack: {
                try await handle.addCandidateTrack(
                    candidateKey: $0,
                    audio: $1,
                    candidate: $2
                )
            },
            dropCandidateTrack: {
                try await handle.dropCandidateTrack(
                    candidateKey: $0,
                    trackId: $1
                )
            },
            candidateRuntime: {
                handle.candidateRuntime(candidateKey: $0)
            },
            subscribeCandidateLiveState: {
                handle.subscribeCandidateLiveState(
                    candidateKey: $0,
                    basis: $1,
                    callback: $2
                )
            },
            candidateSignals: {
                handle.candidateSignals(candidateKey: $0)
                    .map(Signals.init(bridge:))
            },
            startImport: { request in
                try await handle.startImport(
                    candidateKey: request.candidateKey,
                    storageMode: request.storageMode,
                    pin: request.pin
                )
            },
            importReady: { request in
                try await handle.importReady(
                    candidateKey: request.candidateKey,
                    storageMode: request.storageMode,
                    pin: request.pin
                )
            },
            mergeCandidateArtistIdentityConflict: {
                try await handle.mergeCandidateArtistIdentityConflict(
                    candidateKey: $0,
                    survivingArtistId: $1
                )
            },
            setIdentifyAutomatically: {
                try await handle.setIdentifyAutomatically(enabled: $0)
            },
            setPrefillWithFileMetadata: {
                try await handle.setPrefillWithFileMetadata(enabled: $0)
            },
            setMetadataSourceEnabled: {
                try await handle.setMetadataSourceEnabled(
                    source: $0,
                    enabled: $1
                )
            }
        )
    }
}

/// What an importer with no bridge behind it hands back when a surface asks to
/// watch a release's library membership or a candidate's live state: a preview
/// and a test that does not exercise either still render the pane and the
/// rows, which watch what they draw.
private final class InertSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    func cancel() {}
}

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, and commit.
final class Importer: Sendable, Observable {
    private let operations: ImportOperations

    init(
        candidateSourceFolders:
            @escaping @Sendable (String) async throws -> [String] = { _ in
                throw StubError.notImplemented
            },
        combineCandidates:
            @escaping @Sendable ([String]) async throws -> String = { _ in
                throw StubError.notImplemented
            },
        combineFolder:
            @escaping @Sendable (BridgeFolderReleaseDecisionKey) async throws
            -> String = { _ in throw StubError.notImplemented },
        separateCandidate: @escaping @Sendable (String) async throws -> Void =
            { _ in throw StubError.notImplemented },
        chooseFolder:
            @escaping @Sendable (String) async throws -> BridgeChosenFolder = {
                _ in .noReleases
            },
        removeWatchedFolder: @escaping @Sendable (String) async throws -> Void =
            {
                _ in
            },
        refreshWatchedFolder:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        setCandidateSkipped:
            @escaping @Sendable (String, Bool) async throws -> Void = { _, _ in
            },
        setSheetBinding:
            @escaping @Sendable (String, String, String, String?) async throws
            -> Void =
            { _, _, _, _ in },
        applyCandidateExternalMetadata:
            @escaping @Sendable (String, BridgeMetadataProvenance)
            async throws -> UInt64 = { _, _ in
                throw StubError.notImplemented
            },
        applyCandidateFileMetadata:
            @escaping @Sendable (String) async throws -> UInt64 = { _ in
                throw StubError.notImplemented
            },
        resetCandidateSetup:
            @escaping @Sendable (String) async throws -> Void = { _ in
                throw StubError.notImplemented
            },
        clearCandidateMetadata:
            @escaping @Sendable (String) async throws -> UInt64 = { _ in
                throw StubError.notImplemented
            },
        setSheetDisc:
            @escaping @Sendable (String, String, BridgeSheetDisc) async throws
            -> Void = { _, _, _ in },
        setFileRole:
            @escaping @Sendable (String, String, BridgeFileRoleChoice)
            async throws -> Void = { _, _, _ in },
        autoIdentifyRelease:
            @escaping @Sendable (String, String, BridgeLookupChoices) -> Void =
            {
                _,
                _,
                _ in
            },
        cancelAutoIdentify: @escaping @Sendable (String) -> Void = { _ in },
        startCandidateSearch:
            @escaping @Sendable (String, BridgeSearchQuery) -> Void = { _, _ in
            },
        retryCandidateSearch: @escaping @Sendable (String) -> Void = { _ in },
        subscribeLibraryStatuses:
            @escaping @Sendable () -> LibraryStatusQuery = { .inert },
        setCandidateLookupChoices:
            @escaping @Sendable (String, BridgeLookupChoices) async throws ->
            Void = { _, _ in },
        rerunIdentifyForCandidate:
            @escaping @Sendable (String) -> Void = { _ in },
        cancelIdentification:
            @escaping @Sendable ([String]) -> Void = { _ in },
        cancelAllIdentification: @escaping @Sendable () -> Void = {},
        setCandidatePresentation:
            @escaping @Sendable (String, BridgeMetadataPresentation)
            async throws ->
            Void = { _, _ in },
        setCandidateSearchForm:
            @escaping @Sendable (String, BridgeSearchForm) async throws -> Void =
            {
                _,
                _ in
            },
        setCandidatePaneError:
            @escaping @Sendable (String, String?) async throws -> Void = {
                _,
                _ in
            },
        setCandidateCover:
            @escaping @Sendable (String, BridgeCoverSelection) async throws ->
            Void = { _, _ in },
        setCandidateEditField:
            @escaping @Sendable (String, BridgeCandidateEditField, String)
            async throws -> Void = { _, _, _ in },
        setCandidatePressingFact:
            @escaping @Sendable (String, BridgePressingFactEdit) async throws
            -> Void = { _, _ in },
        setCandidateAlbumArtists:
            @escaping @Sendable (String, [BridgeArtistAssignment]) async throws
            -> Void = { _, _ in },
        setCandidateTrackEdit:
            @escaping @Sendable (String, BridgeRawTrackEdit) async throws ->
            Void = { _, _ in },
        addCandidateTrack:
            @escaping @Sendable (String, BridgeAudioFile, BridgeCandidateAsRead)
            async throws -> Void = { _, _, _ in throw StubError.notImplemented
            },
        dropCandidateTrack:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        candidateRuntime:
            @escaping @Sendable (String) -> BridgeCandidateRuntimeSnapshot? = {
                _ in nil
            },
        subscribeCandidateLiveState:
            @escaping @Sendable (
                String, BridgeCandidateActionBasis, CandidateLiveStateCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _ in
                InertSubscription()
            },
        candidateSignals: @escaping @Sendable (String) -> Signals? = { _ in nil
        },
        startImport:
            @escaping @Sendable (ImportCommitRequest) async throws -> Void = {
                _ in
            },
        importReady:
            @escaping @Sendable (ImportCommitRequest) async throws -> Void = {
                _ in
            },
        setIdentifyAutomatically:
            @escaping @MainActor @Sendable (Bool) async throws -> Void = { _ in
            },
        setPrefillWithFileMetadata:
            @escaping @MainActor @Sendable (Bool) async throws -> Void = { _ in
            },
        setMetadataSourceEnabled:
            @escaping @MainActor @Sendable (
                BridgeCatalog, Bool
            ) async throws -> Void = { _, _ in }
    ) {
        operations = ImportOperations(
            candidateSourceFolders: candidateSourceFolders,
            combineCandidates: combineCandidates,
            combineFolder: combineFolder,
            separateCandidate: separateCandidate,
            chooseFolder: chooseFolder,
            removeWatchedFolder: removeWatchedFolder,
            refreshWatchedFolder: refreshWatchedFolder,
            setCandidateSkipped: setCandidateSkipped,
            setSheetBinding: setSheetBinding,
            applyCandidateExternalMetadata: applyCandidateExternalMetadata,
            applyCandidateFileMetadata: applyCandidateFileMetadata,
            resetCandidateSetup: resetCandidateSetup,
            clearCandidateMetadata: clearCandidateMetadata,
            setSheetDisc: setSheetDisc,
            setFileRole: setFileRole,
            autoIdentifyRelease: autoIdentifyRelease,
            cancelAutoIdentify: cancelAutoIdentify,
            startCandidateSearch: startCandidateSearch,
            retryCandidateSearch: retryCandidateSearch,
            subscribeLibraryStatuses: subscribeLibraryStatuses,
            setCandidateLookupChoices: setCandidateLookupChoices,
            rerunIdentifyForCandidate: rerunIdentifyForCandidate,
            cancelIdentification: cancelIdentification,
            cancelAllIdentification: cancelAllIdentification,
            setCandidatePresentation: setCandidatePresentation,
            setCandidateSearchForm: setCandidateSearchForm,
            setCandidatePaneError: setCandidatePaneError,
            setCandidateCover: setCandidateCover,
            setCandidateEditField: setCandidateEditField,
            setCandidatePressingFact: setCandidatePressingFact,
            setCandidateAlbumArtists: setCandidateAlbumArtists,
            setCandidateTrackEdit: setCandidateTrackEdit,
            addCandidateTrack: addCandidateTrack,
            dropCandidateTrack: dropCandidateTrack,
            candidateRuntime: candidateRuntime,
            subscribeCandidateLiveState: subscribeCandidateLiveState,
            candidateSignals: candidateSignals,
            startImport: startImport,
            importReady: importReady,
            mergeCandidateArtistIdentityConflict: { _, _ in
                throw StubError.notImplemented
            },
            setIdentifyAutomatically: setIdentifyAutomatically,
            setPrefillWithFileMetadata: setPrefillWithFileMetadata,
            setMetadataSourceEnabled: setMetadataSourceEnabled
        )
    }

    private init(operations: ImportOperations) {
        self.operations = operations
    }
}

extension Importer {
    func combineCandidates(_ keys: [String]) async throws -> String {
        try await operations.combineCandidates(keys)
    }

    func candidateSourceFolders(_ key: String) async throws -> [String] {
        try await operations.candidateSourceFolders(key)
    }

    func combineFolder(
        _ key: BridgeFolderReleaseDecisionKey
    ) async throws -> String {
        try await operations.combineFolder(key)
    }

    func separateCandidate(_ key: String) async throws {
        try await operations.separateCandidate(key)
    }

    /// Take in the folder at `path` and say, once it has been read, where its
    /// releases stand.
    func chooseFolder(_ path: String) async throws -> BridgeChosenFolder {
        try await operations.chooseFolder(path)
    }

    func removeWatchedFolder(_ path: String) async throws {
        try await operations.removeWatchedFolder(path)
    }

    func refreshWatchedFolder(_ path: String) async throws {
        try await operations.refreshWatchedFolder(path)
    }

    func setCandidateSkipped(_ path: String, _ skipped: Bool) async throws {
        try await operations.setCandidateSkipped(path, skipped)
    }

    func setSheetBinding(
        _ candidateKey: String,
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

    /// Replace the candidate's draft from the release a pick names, claiming
    /// every source that pick carried.
    func applyCandidateExternalMetadata(
        _ candidateKey: String,
        provenance: BridgeMetadataProvenance
    ) async throws -> UInt64 {
        try await operations.applyCandidateExternalMetadata(
            candidateKey,
            provenance
        )
    }

    func applyCandidateFileMetadata(_ candidateKey: String) async throws
        -> UInt64
    {
        try await operations.applyCandidateFileMetadata(candidateKey)
    }

    func resetCandidateSetup(_ candidateKey: String) async throws {
        try await operations.resetCandidateSetup(candidateKey)
    }

    func clearCandidateMetadata(_ candidateKey: String) async throws -> UInt64 {
        try await operations.clearCandidateMetadata(candidateKey)
    }

    func setSheetDisc(
        _ candidateKey: String,
        _ sheetFileId: String,
        _ disc: BridgeSheetDisc
    ) async throws {
        try await operations.setSheetDisc(candidateKey, sheetFileId, disc)
    }

    func setFileRole(
        _ candidateKey: String,
        _ fileId: String,
        _ choice: BridgeFileRoleChoice
    ) async throws {
        try await operations.setFileRole(candidateKey, fileId, choice)
    }

    /// Re-identify a library release. It is not a scanned candidate, so
    /// nothing stores what its run asks about: the sheet holds `choices` and
    /// hands them back with every run it starts.
    func autoIdentifyRelease(
        _ candidateKey: String,
        _ releaseId: String,
        _ choices: BridgeLookupChoices
    ) {
        operations.autoIdentifyRelease(candidateKey, releaseId, choices)
    }

    func cancelAutoIdentify(_ candidateKey: String) {
        operations.cancelAutoIdentify(candidateKey)
    }

    /// Submit a candidate's typed search. Fire-and-forget: every configured
    /// provider is asked at once and each answer lands on the candidate's
    /// runtime, which the pane already watches.
    func startCandidateSearch(
        _ candidateKey: String,
        _ query: BridgeSearchQuery
    ) {
        operations.startCandidateSearch(candidateKey, query)
    }

    /// Re-ask only the providers whose part of the search failed.
    func retryCandidateSearch(_ candidateKey: String) {
        operations.retryCandidateSearch(candidateKey)
    }

    /// Open one live read of library membership for an import pane's offers.
    func subscribeLibraryStatuses() -> LibraryStatusQuery {
        operations.subscribeLibraryStatuses()
    }

    /// Record what a candidate's identification asks about — the whole value,
    /// computed from the detail's current one — and start the run that reads
    /// it.
    func setCandidateLookupChoices(
        _ candidateKey: String,
        _ choices: BridgeLookupChoices
    ) async throws {
        try await operations.setCandidateLookupChoices(candidateKey, choices)
    }

    /// Identify a folder candidate again, over what the candidate says its
    /// lookup asks about and the sources the library asks now. Whatever is
    /// running for it is superseded, and any stored answer is replaced.
    ///
    /// Re-asking the providers that failed is this same command: every lookup
    /// goes out again, and core's response cache answers the ones that had
    /// already succeeded.
    func rerunIdentifyForCandidate(_ candidateKey: String) {
        operations.rerunIdentifyForCandidate(candidateKey)
    }

    /// Take these candidates off the identification queue, waiting or
    /// running. They are left unidentified and are not picked up again on
    /// their own; `rerunIdentifyForCandidate` asks for one again.
    func cancelIdentification(_ candidateKeys: [String]) {
        operations.cancelIdentification(candidateKeys)
    }

    /// Take every candidate off the identification queue.
    func cancelAllIdentification() {
        operations.cancelAllIdentification()
    }

    /// Record which surface the pane's metadata slot shows for a candidate.
    func setCandidatePresentation(
        _ candidateKey: String,
        _ presentation: BridgeMetadataPresentation
    ) async throws {
        try await operations.setCandidatePresentation(
            candidateKey,
            presentation
        )
    }

    /// Record the typed-search form as the person left it.
    func setCandidateSearchForm(
        _ candidateKey: String,
        _ search: BridgeSearchForm
    ) async throws {
        try await operations.setCandidateSearchForm(candidateKey, search)
    }

    /// Record the last command the pane ran for a candidate when it failed,
    /// or clear it for the next command.
    func setCandidatePaneError(_ candidateKey: String, _ error: String?)
        async throws
    {
        try await operations.setCandidatePaneError(candidateKey, error)
    }

    /// Record the cover this candidate commits with.
    func setCandidateCover(
        _ candidateKey: String,
        _ cover: BridgeCoverSelection
    ) async throws {
        try await operations.setCandidateCover(candidateKey, cover)
    }

    /// Record one album-level metadata field as the user left it.
    func setCandidateEditField(
        _ candidateKey: String,
        _ field: BridgeCandidateEditField,
        _ value: String
    ) async throws {
        try await operations.setCandidateEditField(candidateKey, field, value)
    }

    /// Record one choice of what this candidate's pressing is.
    func setCandidatePressingFact(
        _ candidateKey: String,
        _ fact: BridgePressingFactEdit
    ) async throws {
        try await operations.setCandidatePressingFact(candidateKey, fact)
    }

    /// Replace the ordered album artists this candidate commits with.
    func setCandidateAlbumArtists(
        _ candidateKey: String,
        _ assignments: [BridgeArtistAssignment]
    ) async throws {
        try await operations.setCandidateAlbumArtists(candidateKey, assignments)
    }

    /// Record one mapping-table row as the user left it.
    func setCandidateTrackEdit(
        _ candidateKey: String,
        _ track: BridgeRawTrackEdit
    ) async throws {
        try await operations.setCandidateTrackEdit(candidateKey, track)
    }

    /// Include the exact audio offered by the candidate revision the person viewed.
    func addCandidateTrack(
        _ candidateKey: String,
        _ audio: BridgeAudioFile,
        _ candidate: BridgeCandidateAsRead
    ) async throws {
        try await operations.addCandidateTrack(candidateKey, audio, candidate)
    }

    /// Take one mapping-table row out of the import.
    func dropCandidateTrack(
        _ candidateKey: String,
        _ trackId: String
    ) async throws {
        try await operations.dropCandidateTrack(candidateKey, trackId)
    }

    func startImport(_ request: ImportCommitRequest) async throws {
        try await operations.startImport(request)
    }

    /// Import one row of a bulk import of the Ready set. Core refuses a row an
    /// import already owns or identification is still answering, and the
    /// refusal says which.
    func importReady(_ request: ImportCommitRequest) async throws {
        try await operations.importReady(request)
    }

    func mergeCandidateArtistIdentityConflict(
        _ candidateKey: String,
        keeping survivingArtistId: String
    ) async throws {
        try await operations.mergeCandidateArtistIdentityConflict(
            candidateKey,
            survivingArtistId
        )
    }

    @MainActor
    func setIdentifyAutomatically(_ enabled: Bool) async throws {
        try await operations.setIdentifyAutomatically(enabled)
    }

    @MainActor
    func setPrefillWithFileMetadata(_ enabled: Bool) async throws {
        try await operations.setPrefillWithFileMetadata(enabled)
    }

    /// Ask, or stop asking, one metadata source — the same write behind the
    /// Find online header's checkboxes and the Settings ones. Throws when core
    /// refuses, which is when it would leave nothing to ask.
    @MainActor
    func setMetadataSourceEnabled(
        _ source: BridgeCatalog,
        _ enabled: Bool
    ) async throws {
        try await operations.setMetadataSourceEnabled(source, enabled)
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(operations: .live(handle: handle))
    }

}

/// What a view asks about one candidate the moment it appears, having already
/// subscribed to the stream that keeps it current. Every one of these is a
/// read: they start nothing and change nothing.
extension Importer {
    /// What is in flight for one key right now — the read a view does once
    /// when it appears, after it has subscribed to the changes.
    func candidateRuntime(_ candidateKey: String)
        -> BridgeCandidateRuntimeSnapshot?
    {
        operations.candidateRuntime(candidateKey)
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the changes.
    func candidateSignals(_ candidateKey: String) -> Signals? {
        operations.candidateSignals(candidateKey)
    }

    /// What is running for one candidate and the commands its row offers
    /// with it: the value as it stands, then each change. Ending the iteration
    /// ends the subscription.
    func candidateLiveStates(
        _ candidateKey: String,
        basis: BridgeCandidateActionBasis
    ) -> AsyncStream<BridgeCandidateLiveState> {
        AsyncStream { continuation in
            let subscription = operations.subscribeCandidateLiveState(
                candidateKey,
                basis,
                CandidateLiveStateSink { continuation.yield($0) }
            )
            continuation.onTermination = { _ in subscription.cancel() }
        }
    }
}
