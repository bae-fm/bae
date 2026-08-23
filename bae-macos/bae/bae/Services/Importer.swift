import BaeKit
import Foundation

/// What committing a candidate needs from the caller: where its files should
/// live. Everything about the release — the pick, the metadata, the corrected
/// rows, the cover — is stored under the candidate, so the commit reads the
/// very values the pane drew.
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
    let pickCandidateIdentity:
        @Sendable (String, BridgeIdentityPick) async throws -> Void
    let setSheetDisc:
        @Sendable (String, String, BridgeSheetDisc) async throws -> Void
    let setFileRole:
        @Sendable (String, String, BridgeFileRoleChoice) async throws -> Void
    let autoIdentifyFolder: @Sendable (String) -> Void
    let autoIdentifyRelease: @Sendable (String, String) -> Void
    let cancelAutoIdentify: @Sendable (String) -> Void
    let searchForCandidate:
        @Sendable (BridgeSearchQuery) async throws
            -> BridgeCandidateSearchResults
    let subscribeReleaseLibraryStatus:
        @Sendable (
            BridgeMetadataSource, String, String?, ReleaseLibraryStatusCallback
        ) -> any LiveSubscriptionProtocol
    let toggleSignalForCandidate:
        @Sendable (String, BridgeExcludedSignal) -> Void
    let rerunIdentifyForCandidate: @Sendable (String) -> Void
    let setCandidateCover:
        @Sendable (String, BridgeCoverSelection) async throws -> Void
    let setCandidateEditField:
        @Sendable (String, BridgeCandidateEditField, String) async throws ->
            Void
    let setCandidateTrackEdit:
        @Sendable (String, BridgeRawTrackEdit) async throws -> Void
    let dropCandidateTrack: @Sendable (String, String) async throws -> Void
    let claimForPick:
        @Sendable (String, BridgeMetadataResult, BridgeClaimLevel)
            -> BridgeClaimLine?
    let startImport: @Sendable (ImportCommitRequest) async throws -> Void

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
            pickCandidateIdentity: {
                try await handle.pickCandidateIdentity(
                    candidateKey: $0,
                    pick: $1
                )
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
            autoIdentifyFolder: {
                handle.autoIdentifyFolder(candidateKey: $0)
            },
            autoIdentifyRelease: {
                handle.autoIdentifyRelease(candidateKey: $0, releaseId: $1)
            },
            cancelAutoIdentify: {
                handle.cancelAutoIdentify(candidateKey: $0)
            },
            searchForCandidate: {
                try await handle.searchForCandidate(query: $0)
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
            setCandidateTrackEdit: {
                try await handle.setCandidateTrackEdit(
                    candidateKey: $0,
                    track: $1
                )
            },
            dropCandidateTrack: {
                try await handle.dropCandidateTrack(
                    candidateKey: $0,
                    trackId: $1
                )
            },
            claimForPick: {
                handle.claimForPick(candidateKey: $0, result: $1, level: $2)
            },
            startImport: { request in
                try await handle.startImport(
                    candidateKey: request.candidateKey,
                    storageMode: request.storageMode,
                    pin: request.pin
                )
            }
        )
    }
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
        pickCandidateIdentity:
            @escaping @Sendable (String, BridgeIdentityPick) async throws
            -> Void = { _, _ in },
        setSheetDisc:
            @escaping @Sendable (String, String, BridgeSheetDisc) async throws
            -> Void = { _, _, _ in },
        setFileRole:
            @escaping @Sendable (String, String, BridgeFileRoleChoice)
            async throws -> Void = { _, _, _ in },
        autoIdentifyFolder: @escaping @Sendable (String) -> Void = { _ in },
        autoIdentifyRelease: @escaping @Sendable (String, String) -> Void = {
            _,
            _ in
        },
        cancelAutoIdentify: @escaping @Sendable (String) -> Void = { _ in },
        searchForCandidate:
            @escaping @Sendable (BridgeSearchQuery) async throws ->
            BridgeCandidateSearchResults = { _ in throw StubError.notImplemented
            },
        subscribeReleaseLibraryStatus:
            @escaping @Sendable (
                BridgeMetadataSource, String, String?,
                ReleaseLibraryStatusCallback
            ) -> any LiveSubscriptionProtocol = { _, _, _, _ in
                fatalError(
                    "release library status subscription not implemented"
                )
            },
        toggleSignalForCandidate:
            @escaping @Sendable (String, BridgeExcludedSignal) -> Void = {
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
        setCandidateTrackEdit:
            @escaping @Sendable (String, BridgeRawTrackEdit) async throws ->
            Void = { _, _ in },
        dropCandidateTrack:
            @escaping @Sendable (String, String) async throws -> Void = {
                _,
                _ in
            },
        claimForPick:
            @escaping @Sendable (
                String, BridgeMetadataResult, BridgeClaimLevel
            ) -> BridgeClaimLine? = { _, _, _ in nil },
        startImport:
            @escaping @Sendable (ImportCommitRequest) async throws -> Void = {
                _ in
            }
    ) {
        operations = ImportOperations(
            addWatchedFolder: addWatchedFolder,
            removeWatchedFolder: removeWatchedFolder,
            refreshWatchedFolder: refreshWatchedFolder,
            setFolderReleaseDecision: setFolderReleaseDecision,
            setCandidateSkipped: setCandidateSkipped,
            sheetBindingOptions: sheetBindingOptions,
            setSheetBinding: setSheetBinding,
            pickCandidateIdentity: pickCandidateIdentity,
            setSheetDisc: setSheetDisc,
            setFileRole: setFileRole,
            autoIdentifyFolder: autoIdentifyFolder,
            autoIdentifyRelease: autoIdentifyRelease,
            cancelAutoIdentify: cancelAutoIdentify,
            searchForCandidate: searchForCandidate,
            subscribeReleaseLibraryStatus: subscribeReleaseLibraryStatus,
            toggleSignalForCandidate: toggleSignalForCandidate,
            rerunIdentifyForCandidate: rerunIdentifyForCandidate,
            setCandidateCover: setCandidateCover,
            setCandidateEditField: setCandidateEditField,
            setCandidateTrackEdit: setCandidateTrackEdit,
            dropCandidateTrack: dropCandidateTrack,
            claimForPick: claimForPick,
            startImport: startImport
        )
    }

    private init(operations: ImportOperations) {
        self.operations = operations
    }

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

    /// Decide this candidate's identity. Nothing comes back: the per-candidate
    /// read delivers the pane's next value.
    func pickCandidateIdentity(
        _ candidateKey: String,
        _ pick: BridgeIdentityPick
    ) async throws {
        try await operations.pickCandidateIdentity(candidateKey, pick)
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

    func autoIdentifyFolder(_ candidateKey: String) {
        operations.autoIdentifyFolder(candidateKey)
    }

    func autoIdentifyRelease(_ candidateKey: String, _ releaseId: String) {
        operations.autoIdentifyRelease(candidateKey, releaseId)
    }

    func cancelAutoIdentify(_ candidateKey: String) {
        operations.cancelAutoIdentify(candidateKey)
    }

    func searchForCandidate(_ query: BridgeSearchQuery) async throws
        -> BridgeCandidateSearchResults
    {
        try await operations.searchForCandidate(query)
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
        _ signal: BridgeExcludedSignal
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

    /// Record one mapping-table row as the user left it.
    func setCandidateTrackEdit(
        _ candidateKey: String,
        _ track: BridgeRawTrackEdit
    ) async throws {
        try await operations.setCandidateTrackEdit(candidateKey, track)
    }

    /// Take one mapping-table row out of the import.
    func dropCandidateTrack(
        _ candidateKey: String,
        _ trackId: String
    ) async throws {
        try await operations.dropCandidateTrack(candidateKey, trackId)
    }

    func claimForPick(
        _ candidateKey: String,
        _ result: BridgeMetadataResult,
        _ level: BridgeClaimLevel
    ) -> BridgeClaimLine? {
        operations.claimForPick(candidateKey, result, level)
    }

    func startImport(_ request: ImportCommitRequest) async throws {
        try await operations.startImport(request)
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(operations: .live(handle: handle))
    }

}
