import BaeKit
import Foundation

struct ImportCommitRequest: Sendable {
    let candidateKey: String
    let selectedCover: BridgeCoverSelection?
    let storageMode: BridgeStorageMode
    let pin: Bool
    let identityChoice: BridgeIdentityChoice
    let userEdit: BridgeReleaseUserEdit?
}

private struct ImportOperations: Sendable {
    let importCandidates:
        @Sendable () async throws -> BridgeImportCandidatesSnapshot
    let importTriageQueue: @Sendable () async throws -> BridgeTriageQueue
    let candidate:
        @Sendable (String) async throws -> BridgeImportCandidateSnapshot?
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
        @Sendable (String, BridgeIdentityPick) async throws
            -> BridgeDecidedIdentity
    let candidateDecidedIdentity:
        @Sendable (String) async throws -> BridgeDecidedIdentity?
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
    let toggleSignalForCandidate:
        @Sendable (String, BridgeExcludedSignal) -> Void
    let rerunIdentifyForCandidate: @Sendable (String) -> Void
    let candidateMapping: @Sendable (String) throws -> BridgeMappingTable
    let claimForPick:
        @Sendable (String, BridgeMetadataResult, BridgeClaimLevel)
            -> BridgeClaimLine?
    let startImport: @Sendable (ImportCommitRequest) async throws -> Void

    // Flat forwarding from AppHandleProtocol into immutable operation values.
    // swiftlint:disable:next function_body_length
    static func live(handle: any AppHandleProtocol) -> ImportOperations {
        ImportOperations(
            importCandidates: { handle.getImportCandidates() },
            importTriageQueue: { try await handle.getImportTriageQueue() },
            candidate: { handle.getCandidate(key: $0) },
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
            startImport: { request in
                try await handle.startImport(
                    candidateKey: request.candidateKey,
                    selectedCover: request.selectedCover,
                    storageMode: request.storageMode,
                    pin: request.pin,
                    identityChoice: request.identityChoice,
                    userEdit: request.userEdit
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
        importCandidates:
            @escaping @Sendable () async throws ->
            BridgeImportCandidatesSnapshot = {
                throw StubError.notImplemented
            },
        importTriageQueue:
            @escaping @Sendable () async throws -> BridgeTriageQueue = {
                throw StubError.notImplemented
            },
        candidate:
            @escaping @Sendable (String) async throws ->
            BridgeImportCandidateSnapshot? = { _ in
                throw StubError.notImplemented
            },
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
            @escaping @Sendable (ImportCommitRequest) async throws -> Void = {
                _ in
            }
    ) {
        operations = ImportOperations(
            importCandidates: importCandidates,
            importTriageQueue: importTriageQueue,
            candidate: candidate,
            addWatchedFolder: addWatchedFolder,
            removeWatchedFolder: removeWatchedFolder,
            refreshWatchedFolder: refreshWatchedFolder,
            setFolderReleaseDecision: setFolderReleaseDecision,
            setCandidateSkipped: setCandidateSkipped,
            sheetBindingOptions: sheetBindingOptions,
            setSheetBinding: setSheetBinding,
            pickCandidateIdentity: pickCandidateIdentity,
            candidateDecidedIdentity: candidateDecidedIdentity,
            setSheetDisc: setSheetDisc,
            setFileRole: setFileRole,
            autoIdentifyFolder: autoIdentifyFolder,
            autoIdentifyRelease: autoIdentifyRelease,
            cancelAutoIdentify: cancelAutoIdentify,
            searchForCandidate: searchForCandidate,
            toggleSignalForCandidate: toggleSignalForCandidate,
            rerunIdentifyForCandidate: rerunIdentifyForCandidate,
            candidateMapping: candidateMapping,
            claimForPick: claimForPick,
            startImport: startImport
        )
    }

    private init(operations: ImportOperations) {
        self.operations = operations
    }

    func importCandidates() async throws -> BridgeImportCandidatesSnapshot {
        try await operations.importCandidates()
    }

    func importTriageQueue() async throws -> BridgeTriageQueue {
        try await operations.importTriageQueue()
    }

    func candidate(_ key: String) async throws
        -> BridgeImportCandidateSnapshot?
    {
        try await operations.candidate(key)
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

    func pickCandidateIdentity(
        _ candidateKey: String,
        _ pick: BridgeIdentityPick
    )
        async throws -> BridgeDecidedIdentity
    {
        try await operations.pickCandidateIdentity(candidateKey, pick)
    }

    func candidateDecidedIdentity(_ candidateKey: String) async throws
        -> BridgeDecidedIdentity?
    {
        try await operations.candidateDecidedIdentity(candidateKey)
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

    func toggleSignalForCandidate(
        _ candidateKey: String,
        _ signal: BridgeExcludedSignal
    ) {
        operations.toggleSignalForCandidate(candidateKey, signal)
    }

    func rerunIdentifyForCandidate(_ candidateKey: String) {
        operations.rerunIdentifyForCandidate(candidateKey)
    }

    func candidateMapping(_ candidateKey: String) throws -> BridgeMappingTable {
        try operations.candidateMapping(candidateKey)
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
