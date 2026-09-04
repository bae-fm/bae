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

private final class ReleaseLibraryStatusSink: ReleaseLibraryStatusCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeLibraryStatus) -> Void
    private let fail: @MainActor @Sendable (BridgeError) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeLibraryStatus) -> Void,
        fail: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeLibraryStatus) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private struct ImportOperations: Sendable {
    let addWatchedFolder: @Sendable (String) async throws -> Void
    let removeWatchedFolder: @Sendable (String) async throws -> Void
    let refreshWatchedFolder: @Sendable (String) async throws -> Void
    let setFolderReleaseDecision:
        @Sendable (
            BridgeFolderReleaseDecisionKey, BridgeFolderReleaseDecision
        ) async throws -> Void
    let setCandidateSkipped: @Sendable (String, Bool) async throws -> Void
    let sheetBindingOptions:
        @Sendable (String, String) async throws -> [BridgeSheetBindingOption]
    let setSheetBinding:
        @Sendable (String, String, String?) async throws -> Void
    let applyCandidateExternalMetadata:
        @Sendable (String, BridgeMetadataProvenance) async throws -> UInt64
    let applyCandidateFileTags: @Sendable (String) async throws -> UInt64
    let clearCandidateMetadata: @Sendable (String) async throws -> UInt64
    let previewFileTags:
        @Sendable (String) async throws -> BridgeReleaseUserEdit
    let setSheetDisc:
        @Sendable (String, String, BridgeSheetDisc) async throws -> Void
    let setFileRole:
        @Sendable (String, String, BridgeFileRoleChoice) async throws -> Void
    let identifyForExplicitLookup: @MainActor @Sendable (String) -> Void
    let autoIdentifyRelease: @Sendable (String, String) -> Void
    let cancelAutoIdentify: @Sendable (String) -> Void
    let startCandidateSearch: @Sendable (String, BridgeSearchQuery) -> Void
    let retryCandidateSearch: @Sendable (String) -> Void
    let clearCandidateSearch: @Sendable (String) -> Void
    let subscribeReleaseLibraryStatus:
        @Sendable (
            BridgeMetadataSource, String, String?, ReleaseLibraryStatusCallback
        ) -> any LiveSubscriptionProtocol
    let toggleSignalForCandidate:
        @Sendable (String, BridgeSignalToggle) ->
            Void
    let rerunIdentifyForCandidate: @Sendable (String) -> Void
    let setCandidateCover:
        @Sendable (String, BridgeCoverSelection) async throws -> Void
    let setCandidateEditField:
        @Sendable (String, BridgeCandidateEditField, String) async throws ->
            Void
    let setCandidateAlbumArtists:
        @Sendable (String, [BridgeArtistAssignment]) async throws -> Void
    let setCandidateTrackEdit:
        @Sendable (String, BridgeRawTrackEdit) async throws -> Void
    let setCandidateTrackArtists:
        @Sendable (String, [String], BridgeTrackArtistAssignments) async throws
            -> Void
    let dropCandidateTrack: @Sendable (String, String) async throws -> Void
    let candidateRuntime: @Sendable (String) -> BridgeCandidateRuntimeSnapshot?
    let candidateSignals: @Sendable (String) -> Signals?
    let startImport: @Sendable (ImportCommitRequest) async throws -> Void
    let mergeCandidateArtistIdentityConflict:
        @Sendable (String, String) async throws -> Void
    let setIdentifyAutomatically: @MainActor @Sendable (Bool) throws -> Void
    let setDefaultMetadataSource:
        @MainActor @Sendable (BridgeDefaultImportMetadataSource) throws -> Void

    // Flat forwarding from AppHandleProtocol into immutable operation values.
    // swiftlint:disable:next function_body_length
    static func live(handle: any AppHandleProtocol) -> ImportOperations {
        ImportOperations(
            addWatchedFolder: {
                try await handle.addWatchedFolder(path: $0)
            },
            removeWatchedFolder: {
                try await handle.removeWatchedFolder(path: $0)
            },
            refreshWatchedFolder: {
                try await handle.refreshWatchedFolder(path: $0)
            },
            setFolderReleaseDecision: {
                try await handle.setFolderReleaseDecision(
                    key: $0,
                    decision: $1
                )
            },
            setCandidateSkipped: {
                try await handle.setCandidateSkipped(path: $0, skipped: $1)
            },
            sheetBindingOptions: {
                try await handle.sheetBindingOptions(
                    candidateKey: $0,
                    sheetFileId: $1
                )
            },
            setSheetBinding: {
                try await handle.setSheetBinding(
                    candidateKey: $0,
                    sheetFileId: $1,
                    audioFileId: $2
                )
            },
            applyCandidateExternalMetadata: {
                try await handle.selectCandidateMetadataProvenance(
                    candidateKey: $0,
                    provenance: $1
                )
            },
            applyCandidateFileTags: {
                try await handle.selectCandidateMetadataProvenance(
                    candidateKey: $0,
                    provenance: .fileTags
                )
            },
            clearCandidateMetadata: {
                try await handle.clearCandidateMetadata(candidateKey: $0)
            },
            previewFileTags: {
                try await handle.previewFileTagsForFolder(candidateKey: $0)
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
            identifyForExplicitLookup: {
                handle.identifyFolderForLookup(candidateKey: $0)
            },
            autoIdentifyRelease: {
                handle.autoIdentifyRelease(candidateKey: $0, releaseId: $1)
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
            clearCandidateSearch: {
                handle.clearCandidateSearch(candidateKey: $0)
            },
            subscribeReleaseLibraryStatus: {
                handle.subscribeReleaseLibraryStatus(
                    source: $0,
                    releaseId: $1,
                    sourceGroupId: $2,
                    callback: $3
                )
            },
            toggleSignalForCandidate: {
                handle.toggleSignalForCandidate(candidateKey: $0, signal: $1)
            },
            rerunIdentifyForCandidate: {
                handle.rerunIdentifyForCandidate(candidateKey: $0)
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
            setCandidateTrackArtists: {
                try await handle.setCandidateTrackArtists(
                    candidateKey: $0,
                    trackIds: $1,
                    assignments: $2
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
            mergeCandidateArtistIdentityConflict: {
                try await handle.mergeCandidateArtistIdentityConflict(
                    candidateKey: $0,
                    survivingArtistId: $1
                )
            },
            setIdentifyAutomatically: {
                try handle.setIdentifyAutomatically(enabled: $0)
            },
            setDefaultMetadataSource: {
                try handle.setDefaultImportMetadataSource(source: $0)
            }
        )
    }
}

/// What an importer with no bridge behind it hands back when a surface asks to
/// watch a release's library membership: a preview and a test that does not
/// exercise membership still render the pane, which watches every release it
/// offers.
private final class InertLibraryStatusSubscription: LiveSubscriptionProtocol,
    @unchecked Sendable
{
    func cancel() {}
}

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, and commit.
final class Importer: Sendable, Observable {
    private let operations: ImportOperations

    init(
        addWatchedFolder: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        removeWatchedFolder: @escaping @Sendable (String) async throws -> Void =
            {
                _ in
            },
        refreshWatchedFolder:
            @escaping @Sendable (String) async throws -> Void = { _ in },
        setFolderReleaseDecision:
            @escaping @Sendable (
                BridgeFolderReleaseDecisionKey, BridgeFolderReleaseDecision
            ) async throws -> Void = { _, _ in },
        setCandidateSkipped:
            @escaping @Sendable (String, Bool) async throws -> Void = { _, _ in
            },
        sheetBindingOptions:
            @escaping @Sendable (String, String) async throws ->
            [BridgeSheetBindingOption] = { _, _ in [] },
        setSheetBinding:
            @escaping @Sendable (String, String, String?) async throws -> Void =
            { _, _, _ in },
        applyCandidateExternalMetadata:
            @escaping @Sendable (String, BridgeMetadataProvenance)
            async throws -> UInt64 = { _, _ in
                throw StubError.notImplemented
            },
        applyCandidateFileTags:
            @escaping @Sendable (String) async throws -> UInt64 = { _ in
                throw StubError.notImplemented
            },
        clearCandidateMetadata:
            @escaping @Sendable (String) async throws -> UInt64 = { _ in
                throw StubError.notImplemented
            },
        previewFileTags:
            @escaping @Sendable (String) async throws -> BridgeReleaseUserEdit =
            { _ in throw StubError.notImplemented },
        setSheetDisc:
            @escaping @Sendable (String, String, BridgeSheetDisc) async throws
            -> Void = { _, _, _ in },
        setFileRole:
            @escaping @Sendable (String, String, BridgeFileRoleChoice)
            async throws -> Void = { _, _, _ in },
        identifyForExplicitLookup:
            @escaping @MainActor @Sendable (String) -> Void = { _ in },
        autoIdentifyRelease: @escaping @Sendable (String, String) -> Void = {
            _,
            _ in
        },
        cancelAutoIdentify: @escaping @Sendable (String) -> Void = { _ in },
        startCandidateSearch:
            @escaping @Sendable (String, BridgeSearchQuery) -> Void = { _, _ in
            },
        retryCandidateSearch: @escaping @Sendable (String) -> Void = { _ in },
        clearCandidateSearch: @escaping @Sendable (String) -> Void = { _ in },
        subscribeReleaseLibraryStatus:
            @escaping @Sendable (
                BridgeMetadataSource, String, String?,
                ReleaseLibraryStatusCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                InertLibraryStatusSubscription()
            },
        toggleSignalForCandidate:
            @escaping @Sendable (String, BridgeSignalToggle) -> Void = {
                _,
                _ in
            },
        rerunIdentifyForCandidate:
            @escaping @Sendable (String) -> Void = { _ in },
        setCandidateCover:
            @escaping @Sendable (String, BridgeCoverSelection) async throws ->
            Void = { _, _ in },
        setCandidateEditField:
            @escaping @Sendable (String, BridgeCandidateEditField, String)
            async throws -> Void = { _, _, _ in },
        setCandidateAlbumArtists:
            @escaping @Sendable (String, [BridgeArtistAssignment]) async throws
            -> Void = { _, _ in },
        setCandidateTrackEdit:
            @escaping @Sendable (String, BridgeRawTrackEdit) async throws ->
            Void = { _, _ in },
        setCandidateTrackArtists:
            @escaping @Sendable (
                String, [String], BridgeTrackArtistAssignments
            ) async throws -> Void = { _, _, _ in },
        dropCandidateTrack:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        candidateRuntime:
            @escaping @Sendable (String) -> BridgeCandidateRuntimeSnapshot? = {
                _ in nil
            },
        candidateSignals: @escaping @Sendable (String) -> Signals? = { _ in nil
        },
        startImport:
            @escaping @Sendable (ImportCommitRequest) async throws -> Void = {
                _ in
            },
        setIdentifyAutomatically:
            @escaping @MainActor @Sendable (Bool) throws -> Void = { _ in },
        setDefaultMetadataSource:
            @escaping @MainActor @Sendable (
                BridgeDefaultImportMetadataSource
            )
            throws -> Void =
            { _ in }
    ) {
        operations = ImportOperations(
            addWatchedFolder: addWatchedFolder,
            removeWatchedFolder: removeWatchedFolder,
            refreshWatchedFolder: refreshWatchedFolder,
            setFolderReleaseDecision: setFolderReleaseDecision,
            setCandidateSkipped: setCandidateSkipped,
            sheetBindingOptions: sheetBindingOptions,
            setSheetBinding: setSheetBinding,
            applyCandidateExternalMetadata: applyCandidateExternalMetadata,
            applyCandidateFileTags: applyCandidateFileTags,
            clearCandidateMetadata: clearCandidateMetadata,
            previewFileTags: previewFileTags,
            setSheetDisc: setSheetDisc,
            setFileRole: setFileRole,
            identifyForExplicitLookup: identifyForExplicitLookup,
            autoIdentifyRelease: autoIdentifyRelease,
            cancelAutoIdentify: cancelAutoIdentify,
            startCandidateSearch: startCandidateSearch,
            retryCandidateSearch: retryCandidateSearch,
            clearCandidateSearch: clearCandidateSearch,
            subscribeReleaseLibraryStatus: subscribeReleaseLibraryStatus,
            toggleSignalForCandidate: toggleSignalForCandidate,
            rerunIdentifyForCandidate: rerunIdentifyForCandidate,
            setCandidateCover: setCandidateCover,
            setCandidateEditField: setCandidateEditField,
            setCandidateAlbumArtists: setCandidateAlbumArtists,
            setCandidateTrackEdit: setCandidateTrackEdit,
            setCandidateTrackArtists: setCandidateTrackArtists,
            dropCandidateTrack: dropCandidateTrack,
            candidateRuntime: candidateRuntime,
            candidateSignals: candidateSignals,
            startImport: startImport,
            mergeCandidateArtistIdentityConflict: { _, _ in
                throw StubError.notImplemented
            },
            setIdentifyAutomatically: setIdentifyAutomatically,
            setDefaultMetadataSource: setDefaultMetadataSource
        )
    }

    private init(operations: ImportOperations) {
        self.operations = operations
    }
}

extension Importer {
    func addWatchedFolder(_ path: String) async throws {
        try await operations.addWatchedFolder(path)
    }

    func removeWatchedFolder(_ path: String) async throws {
        try await operations.removeWatchedFolder(path)
    }

    func refreshWatchedFolder(_ path: String) async throws {
        try await operations.refreshWatchedFolder(path)
    }

    func setFolderReleaseDecision(
        _ key: BridgeFolderReleaseDecisionKey,
        _ decision: BridgeFolderReleaseDecision
    ) async throws {
        try await operations.setFolderReleaseDecision(key, decision)
    }

    func setCandidateSkipped(_ path: String, _ skipped: Bool) async throws {
        try await operations.setCandidateSkipped(path, skipped)
    }

    func sheetBindingOptions(_ candidateKey: String, _ sheetFileId: String)
        async throws -> [BridgeSheetBindingOption]
    {
        try await operations.sheetBindingOptions(candidateKey, sheetFileId)
    }

    func setSheetBinding(
        _ candidateKey: String,
        _ sheetFileId: String,
        _ audioFileId: String?
    ) async throws {
        try await operations.setSheetBinding(
            candidateKey,
            sheetFileId,
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

    func applyCandidateFileTags(_ candidateKey: String) async throws -> UInt64 {
        try await operations.applyCandidateFileTags(candidateKey)
    }

    func clearCandidateMetadata(_ candidateKey: String) async throws -> UInt64 {
        try await operations.clearCandidateMetadata(candidateKey)
    }

    /// Read the candidate's file-tag snapshot without choosing it as the seed.
    func previewFileTags(_ candidateKey: String) async throws
        -> BridgeReleaseUserEdit
    {
        try await operations.previewFileTags(candidateKey)
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

    @MainActor
    func identifyForExplicitLookup(_ candidateKey: String) {
        operations.identifyForExplicitLookup(candidateKey)
    }

    func autoIdentifyRelease(_ candidateKey: String, _ releaseId: String) {
        operations.autoIdentifyRelease(candidateKey, releaseId)
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

    /// Drop a candidate's search, so its result area goes back to whatever
    /// identification has to say.
    func clearCandidateSearch(_ candidateKey: String) {
        operations.clearCandidateSearch(candidateKey)
    }

    func subscribeReleaseLibraryStatus(
        source: BridgeMetadataSource,
        releaseId: String,
        sourceGroupId: String?,
        onValue: @escaping @MainActor @Sendable (BridgeLibraryStatus) -> Void,
        onError: @escaping @MainActor @Sendable (BridgeError) -> Void
    ) -> any LiveSubscriptionProtocol {
        operations.subscribeReleaseLibraryStatus(
            source,
            releaseId,
            sourceGroupId,
            ReleaseLibraryStatusSink(apply: onValue, fail: onError)
        )
    }

    func toggleSignalForCandidate(
        _ candidateKey: String,
        _ signal: BridgeSignalToggle
    ) {
        operations.toggleSignalForCandidate(candidateKey, signal)
    }

    func rerunIdentifyForCandidate(_ candidateKey: String) {
        operations.rerunIdentifyForCandidate(candidateKey)
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

    /// Replace the artist assignments of the named mapping-table rows in one
    /// commit, so a spreadsheet fill cannot stop halfway down the selection.
    func setCandidateTrackArtists(
        _ candidateKey: String,
        _ trackIds: [String],
        _ assignments: BridgeTrackArtistAssignments
    ) async throws {
        try await operations.setCandidateTrackArtists(
            candidateKey,
            trackIds,
            assignments
        )
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
    func setIdentifyAutomatically(_ enabled: Bool) throws {
        try operations.setIdentifyAutomatically(enabled)
    }

    @MainActor
    func setDefaultMetadataSource(
        _ source: BridgeDefaultImportMetadataSource
    ) throws {
        try operations.setDefaultMetadataSource(source)
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
}
