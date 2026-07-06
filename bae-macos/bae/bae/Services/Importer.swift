import Foundation

/// Import-flow operations: watched-folder management, scan, identify,
/// candidate search, signal dismissal, file-tag preview, commit, and the
/// duplicate-name check.
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
    let searchForCandidate:
        @Sendable (_ query: BridgeSearchQuery) async throws ->
            BridgeCandidateSearchResults
    let toggleSignalForCandidate:
        @Sendable (_ candidateKey: String, _ signal: BridgeExcludedSignal) ->
            Void
    let rerunIdentifyForCandidate: @Sendable (_ candidateKey: String) -> Void
    let previewFileTagsForFolder:
        @Sendable (_ folderPath: String) async throws -> BridgeReleaseUserEdit
    let startImport:
        @Sendable (
            _ candidateKey: String, _ folderPath: String,
            _ selectedCover: BridgeCoverSelection?,
            _ storageMode: BridgeStorageMode,
            _ pin: Bool,
            _ identityChoice: BridgeIdentityChoice,
            _ userEdit: BridgeReleaseUserEdit?
        ) throws -> Void
    let isSourceFolderNameImported:
        @Sendable (_ name: String) async throws
            -> Bool

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
        startImport:
            @escaping @Sendable (
                String, String, BridgeCoverSelection?, BridgeStorageMode, Bool,
                BridgeIdentityChoice, BridgeReleaseUserEdit?
            ) throws -> Void = { _, _, _, _, _, _, _ in },
        isSourceFolderNameImported:
            @escaping @Sendable (String) async throws -> Bool = { _ in false }
    ) {
        self.addWatchedFolder = addWatchedFolder
        self.removeWatchedFolder = removeWatchedFolder
        self.setCandidateSkipped = setCandidateSkipped
        self.scanWatchedFolders = scanWatchedFolders
        self.autoIdentifyFolder = autoIdentifyFolder
        self.autoIdentifyRelease = autoIdentifyRelease
        self.searchForCandidate = searchForCandidate
        self.toggleSignalForCandidate = toggleSignalForCandidate
        self.rerunIdentifyForCandidate = rerunIdentifyForCandidate
        self.previewFileTagsForFolder = previewFileTagsForFolder
        self.startImport = startImport
        self.isSourceFolderNameImported = isSourceFolderNameImported
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
            },
            isSourceFolderNameImported: {
                try await handle.isSourceFolderNameImported(name: $0)
            }
        )
    }

    // periphery:ignore
    static let stub = Importer()
}
