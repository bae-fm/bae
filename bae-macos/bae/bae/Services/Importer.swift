import Foundation

/// Import-flow operations: scan, identify, candidate search, text
/// scan, signal dismissal, file-tag preview, commit, candidate list
/// mutations, duplicate-name check.
final class Importer: Sendable, Observable {
    let enqueueFolderScan:
        @Sendable (_ path: String, _ clearFirst: Bool) throws -> Void
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
            _ identityChoice: BridgeIdentityChoice,
            _ userEdit: BridgeReleaseUserEdit?
        ) throws -> Void
    let clearAllCandidates: @Sendable () -> Void
    let removeCandidate: @Sendable (_ candidateKey: String) -> Void
    let isSourceFolderNameImported: @Sendable (_ name: String) throws -> Bool

    init(
        enqueueFolderScan: @escaping @Sendable (String, Bool) throws -> Void = {
            _,
            _ in
        },
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
                String, String, BridgeCoverSelection?, BridgeStorageMode,
                BridgeIdentityChoice, BridgeReleaseUserEdit?
            ) throws -> Void = { _, _, _, _, _, _ in },
        clearAllCandidates: @escaping @Sendable () -> Void = {},
        removeCandidate: @escaping @Sendable (String) -> Void = { _ in },
        isSourceFolderNameImported:
            @escaping @Sendable (String) throws -> Bool = { _ in false }
    ) {
        self.enqueueFolderScan = enqueueFolderScan
        self.autoIdentifyFolder = autoIdentifyFolder
        self.autoIdentifyRelease = autoIdentifyRelease
        self.searchForCandidate = searchForCandidate
        self.toggleSignalForCandidate = toggleSignalForCandidate
        self.rerunIdentifyForCandidate = rerunIdentifyForCandidate
        self.previewFileTagsForFolder = previewFileTagsForFolder
        self.startImport = startImport
        self.clearAllCandidates = clearAllCandidates
        self.removeCandidate = removeCandidate
        self.isSourceFolderNameImported = isSourceFolderNameImported
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            enqueueFolderScan: {
                try handle.enqueueFolderScan(path: $0, clearFirst: $1)
            },
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
                    identityChoice: $4,
                    userEdit: $5
                )
            },
            clearAllCandidates: { handle.clearAllCandidates() },
            removeCandidate: { handle.removeCandidate(candidateKey: $0) },
            isSourceFolderNameImported: {
                try handle.isSourceFolderNameImported(name: $0)
            }
        )
    }

    // periphery:ignore
    static let stub = Importer()
}
