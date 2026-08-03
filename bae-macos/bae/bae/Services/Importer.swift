import BaeKit
import Foundation

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, and commit.
final class Importer: Sendable, Observable {
    /// Add a folder to watch for imports: persist it and scan it. Already-watched
    /// folders are left as-is.
    let addWatchedFolder: @Sendable (_ path: String) async throws -> Void
    /// Stop watching a folder; its candidates drop out of the list.
    let removeWatchedFolder: @Sendable (_ path: String) async throws -> Void
    let refreshWatchedFolder: @Sendable (_ path: String) async throws -> Void
    let setFolderReleaseDecision:
        @Sendable (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) async throws -> Void
    /// Mark a candidate skipped (or unskipped); the import view re-tabs the row.
    let setCandidateSkipped:
        @Sendable (_ path: String, _ skipped: Bool) async throws -> Void
    /// What a candidate's track sheet may be bound to: the folder's audio, each
    /// already offered or refused with a reason. Core probes to decide, so ask
    /// when the pane opens rather than holding it with the candidate.
    let sheetBindingOptions:
        @Sendable (_ candidateKey: String, _ sheetFileId: String) async throws
            -> [BridgeSheetBindingOption]
    /// Name the audio a track sheet describes, or clear the binding with `nil`.
    /// Core persists the decision and drops the candidate's stored identify
    /// verdict, so the candidate invalidation brings back both the new roles and
    /// a fresh identification.
    let setSheetBinding:
        @Sendable (
            _ candidateKey: String, _ sheetFileId: String,
            _ audioFileId: String?
        ) async throws -> Void
    /// Say which disc of the release a track sheet's entries are, or take them
    /// out of the tracklist with `.ignored`. Cue filenames are arbitrary, so
    /// the assignment is the truth about which cue is which disc. Core persists
    /// it and drops the candidate's stored identify verdict, because a
    /// re-assigned sheet is a different tracklist.
    let setSheetDisc:
        @Sendable (
            _ candidateKey: String, _ sheetFileId: String,
            _ disc: BridgeSheetDisc
        ) async throws -> Void
    /// Decide the candidate's identity: persist the choice and come back with
    /// everything the pane seeds from it — the same payload
    /// `candidateDecidedIdentity` serves, so a fresh launch renders exactly
    /// what the click rendered.
    let pickCandidateIdentity:
        @Sendable (
            _ candidateKey: String, _ pick: BridgeIdentityPick
        ) async throws -> BridgeDecidedIdentity
    /// The candidate's decided identity read back, or `nil` while nothing is
    /// decided — what selecting a row asks.
    let candidateDecidedIdentity:
        @Sendable (_ candidateKey: String) async throws
            -> BridgeDecidedIdentity?
    /// Put one of a candidate's files in a role, or put it back in the one the
    /// scan proposed. `choice` must be one of that file's `alternatives`. Core
    /// persists the decision — taking a file out of the tracklist is a fact
    /// about the folder, not a list edit — drops the stored identify verdict,
    /// and emits a candidate invalidation carrying the new roles.
    let setFileRole:
        @Sendable (
            _ candidateKey: String, _ fileId: String,
            _ choice: BridgeFileRoleChoice
        ) async throws -> Void
    let autoIdentifyFolder: @Sendable (_ candidateKey: String) -> Void
    let autoIdentifyRelease:
        @Sendable (_ candidateKey: String, _ releaseId: String) -> Void
    /// Stop a candidate's identify pipeline (driver + in-flight artwork OCR).
    let cancelAutoIdentify: @Sendable (_ candidateKey: String) -> Void
    let searchForCandidate:
        @Sendable (_ query: BridgeSearchQuery) async throws ->
            BridgeCandidateSearchResults
    let toggleSignalForCandidate:
        @Sendable (_ candidateKey: String, _ signal: BridgeExcludedSignal) ->
            Void
    let rerunIdentifyForCandidate: @Sendable (_ candidateKey: String) -> Void
    /// The mapping table for a folder nobody has picked a release for: every
    /// source unit it offers, with what each becomes left open.
    let candidateMapping:
        @Sendable (_ candidateKey: String) throws -> BridgeMappingTable
    /// What holding `result` at `level` under a candidate claims, and where
    /// its metadata comes from. The re-identify sheet's path: it commits
    /// straight from the picked row, so it never prefetches. The import
    /// confirm pane gets the same claim back inside the decided-identity
    /// answer instead.
    let claimForPick:
        @Sendable (
            _ candidateKey: String, _ result: BridgeMetadataResult,
            _ level: BridgeClaimLevel
        ) -> BridgeClaimLine?
    /// Async because core claims the candidate for the import before the
    /// command is queued, and that claim is taken under the same lock a
    /// background verdict write holds — which is what keeps the queue sweep
    /// from answering a candidate you have just committed to importing.
    let startImport:
        @Sendable (
            _ candidateKey: String,
            _ selectedCover: BridgeCoverSelection?,
            _ storageMode: BridgeStorageMode,
            _ pin: Bool,
            _ identityChoice: BridgeIdentityChoice,
            _ userEdit: BridgeReleaseUserEdit?
        ) async throws -> Void

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
            -> BridgeDecidedIdentity = { _, _ in
                throw StubError.notImplemented
            },
        candidateDecidedIdentity:
            @escaping @Sendable (String) async throws
            -> BridgeDecidedIdentity? = { _ in nil },
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
        toggleSignalForCandidate:
            @escaping @Sendable (String, BridgeExcludedSignal) -> Void = {
                _,
                _ in
            },
        rerunIdentifyForCandidate:
            @escaping @Sendable (String) -> Void = { _ in },
        candidateMapping:
            @escaping @Sendable (String) throws -> BridgeMappingTable = { _ in
                throw StubError.notImplemented
            },
        claimForPick:
            @escaping @Sendable (
                String, BridgeMetadataResult, BridgeClaimLevel
            ) -> BridgeClaimLine? = { _, _, _ in nil },
        startImport:
            @escaping @Sendable (
                String, BridgeCoverSelection?, BridgeStorageMode, Bool,
                BridgeIdentityChoice, BridgeReleaseUserEdit?
            ) async throws -> Void = { _, _, _, _, _, _ in }
    ) {
        self.addWatchedFolder = addWatchedFolder
        self.removeWatchedFolder = removeWatchedFolder
        self.refreshWatchedFolder = refreshWatchedFolder
        self.setFolderReleaseDecision = setFolderReleaseDecision
        self.setCandidateSkipped = setCandidateSkipped
        self.sheetBindingOptions = sheetBindingOptions
        self.setSheetBinding = setSheetBinding
        self.pickCandidateIdentity = pickCandidateIdentity
        self.candidateDecidedIdentity = candidateDecidedIdentity
        self.setSheetDisc = setSheetDisc
        self.setFileRole = setFileRole
        self.autoIdentifyFolder = autoIdentifyFolder
        self.autoIdentifyRelease = autoIdentifyRelease
        self.cancelAutoIdentify = cancelAutoIdentify
        self.searchForCandidate = searchForCandidate
        self.toggleSignalForCandidate = toggleSignalForCandidate
        self.rerunIdentifyForCandidate = rerunIdentifyForCandidate
        self.candidateMapping = candidateMapping
        self.claimForPick = claimForPick
        self.startImport = startImport
    }

    // Flat 1:1 argument forwarding from `AppHandleProtocol` to this type's
    // closures; its length tracks the number of import-flow calls, not logical
    // complexity.
    // swiftlint:disable:next function_body_length
    convenience init(handle: any AppHandleProtocol) {
        self.init(
            addWatchedFolder: { try await handle.addWatchedFolder(path: $0) },
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
            candidateDecidedIdentity: {
                try await handle.candidateDecidedIdentity(candidateKey: $0)
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
            toggleSignalForCandidate: {
                handle.toggleSignalForCandidate(candidateKey: $0, signal: $1)
            },
            rerunIdentifyForCandidate: {
                handle.rerunIdentifyForCandidate(candidateKey: $0)
            },
            candidateMapping: {
                try handle.candidateMapping(candidateKey: $0)
            },
            claimForPick: {
                handle.claimForPick(candidateKey: $0, result: $1, level: $2)
            },
            startImport: {
                try await handle.startImport(
                    candidateKey: $0,
                    selectedCover: $1,
                    storageMode: $2,
                    pin: $3,
                    identityChoice: $4,
                    userEdit: $5
                )
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static let stub = Importer()
    #endif
}
