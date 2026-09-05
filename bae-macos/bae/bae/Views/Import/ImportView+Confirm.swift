import BaeKit
import SwiftUI
import os.log

private let importConfirmLogger = Logger.bae("ImportConfirm")

// MARK: - Commit

extension ImportView {
    /// Commit the selected candidate. Nothing about the release is sent: the
    /// pick, the metadata typed over it, the corrected rows and the chosen
    /// cover are all stored under the candidate, so the commit consumes the
    /// very values this pane drew. Only where the files should live is this
    /// view's to say.
    ///
    /// A failure — a folder that moved, an album title left empty — lands on
    /// the candidate's banner and the fields stay as they were.
    func commitConfirmedImport(candidate: Candidate) {
        guard case .folder = candidate.source else {
            return
        }
        let request = ImportCommitRequest(
            candidateKey: candidate.key,
            storageMode: configStore.config.importStorageMode(
                cloud: storageCloud
            ),
            pin: storagePinned,
        )
        runCandidateMutation(candidate: candidate) {
            try await importer.startImport(request)
        }
    }

    /// Consolidate the two library artist rows named by the persisted import
    /// conflict. The candidate subscription removes the conflict banner only
    /// after core commits every reference move and deletes the absorbed row.
    func mergeArtistIdentityConflict(
        candidate: Candidate,
        keeping survivingArtistId: String
    ) {
        runCandidateMutation(candidate: candidate) {
            try await importer.mergeCandidateArtistIdentityConflict(
                candidate.key,
                keeping: survivingArtistId
            )
        }
    }

    private func runCandidateMutation(
        candidate: Candidate,
        operation: @escaping @MainActor () async throws -> Void
    ) {
        // Start each attempt from a clean error state so a prior failed
        // command's banner does not linger over a succeeding retry.
        importStore.clearPaneError(forKey: candidate.key)
        candidateMutationTasks[candidate.key]?.cancel()
        candidateMutationTasks[candidate.key] = Task { @MainActor in
            do {
                try await operation()
            }
            catch is CancellationError {
                importConfirmLogger.debug(
                    "candidate command cancelled for \(candidate.key)"
                )
            }
            catch {
                if let line = error.displayLine {
                    importStore.recordPaneError(line, forKey: candidate.key)
                }
                else {
                    uiStore.showError(error)
                }
            }
        }
    }
}
