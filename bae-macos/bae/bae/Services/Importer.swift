import BaeKit
import Foundation

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, and commit.
final class Importer: Sendable, Observable {
    /// Add a folder to watch for imports: persist it and scan it. Already-watched
    /// folders are left as-is.
    let addWatchedFolder: @Sendable (_ path: String) throws -> Void
    /// Stop watching a folder; its candidates drop out of the list.
    let removeWatchedFolder: @Sendable (_ path: String) throws -> Void
    /// Mark a candidate skipped (or unskipped); the import view re-tabs the row.
    let setCandidateSkipped:
        @Sendable (_ path: String, _ skipped: Bool) throws -> Void
    /// Scan every watched folder, streaming candidates back as events. Called
    /// when the import view appears to populate the list.
    let scanWatchedFolders: @Sendable () throws -> Void
    let autoIdentifyFolder:
        @Sendable (_ candidateKey: String, _ folderPath: String) -> Void
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
    let previewFileTagsForFolder:
        @Sendable (_ folderPath: String) async throws -> BridgeReleaseUserEdit
    /// What picking `result` under a candidate claims, and where its metadata
    /// comes from. The re-identify sheet's path: it commits straight from the
    /// picked row, so it never prefetches. The import confirm pane gets the
    /// same claim back from `Library.prefetchRelease` instead.
    let claimForPick:
        @Sendable (_ candidateKey: String, _ result: BridgeMetadataResult) ->
            BridgeClaimLine?
    let startImport:
        @Sendable (
            _ candidateKey: String, _ folderPath: String,
            _ selectedCover: BridgeCoverSelection?,
            _ storageMode: BridgeStorageMode,
            _ pin: Bool,
            _ identityChoice: BridgeIdentityChoice,
            _ userEdit: BridgeReleaseUserEdit?
        ) throws -> Void

    init(
        addWatchedFolder: @escaping @Sendable (String) throws -> Void = { _ in
        },
        removeWatchedFolder: @escaping @Sendable (String) throws -> Void = {
            _ in
        },
        setCandidateSkipped:
            @escaping @Sendable (String, Bool) throws -> Void = { _, _ in },
        scanWatchedFolders: @escaping @Sendable () throws -> Void = {},
        autoIdentifyFolder: @escaping @Sendable (String, String) -> Void = {
            _,
            _ in
        },
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
        previewFileTagsForFolder:
            @escaping @Sendable (String) async throws -> BridgeReleaseUserEdit =
            { _ in throw StubError.notImplemented },
        claimForPick:
            @escaping @Sendable (String, BridgeMetadataResult) ->
            BridgeClaimLine? = { _, _ in nil },
        startImport:
            @escaping @Sendable (
                String, String, BridgeCoverSelection?, BridgeStorageMode, Bool,
                BridgeIdentityChoice, BridgeReleaseUserEdit?
            ) throws -> Void = { _, _, _, _, _, _, _ in }
    ) {
        self.addWatchedFolder = addWatchedFolder
        self.removeWatchedFolder = removeWatchedFolder
        self.setCandidateSkipped = setCandidateSkipped
        self.scanWatchedFolders = scanWatchedFolders
        self.autoIdentifyFolder = autoIdentifyFolder
        self.autoIdentifyRelease = autoIdentifyRelease
        self.cancelAutoIdentify = cancelAutoIdentify
        self.searchForCandidate = searchForCandidate
        self.toggleSignalForCandidate = toggleSignalForCandidate
        self.rerunIdentifyForCandidate = rerunIdentifyForCandidate
        self.previewFileTagsForFolder = previewFileTagsForFolder
        self.claimForPick = claimForPick
        self.startImport = startImport
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            addWatchedFolder: { try handle.addWatchedFolder(path: $0) },
            removeWatchedFolder: { try handle.removeWatchedFolder(path: $0) },
            setCandidateSkipped: {
                try handle.setCandidateSkipped(path: $0, skipped: $1)
            },
            scanWatchedFolders: { try handle.scanWatchedFolders() },
            autoIdentifyFolder: {
                handle.autoIdentifyFolder(candidateKey: $0, folderPath: $1)
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
            previewFileTagsForFolder: {
                try await handle.previewFileTagsForFolder(folderPath: $0)
            },
            claimForPick: {
                handle.claimForPick(candidateKey: $0, result: $1)
            },
            startImport: {
                try handle.startImport(
                    candidateKey: $0,
                    folderPath: $1,
                    selectedCover: $2,
                    storageMode: $3,
                    pin: $4,
                    identityChoice: $5,
                    userEdit: $6
                )
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        static let stub = Importer()
    #endif
}
