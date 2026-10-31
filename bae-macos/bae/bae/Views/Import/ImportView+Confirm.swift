import BaeKit
import SwiftUI
import os.log

private let importConfirmLogger = Logger.bae("ImportConfirm")

// MARK: - Commit

extension ImportView {
    /// Commit the selected candidate; core holds everything it commits.
    ///
    /// A failure — a folder that moved, an album title left empty — lands on
    /// the candidate's banner and the fields stay as they were.
    func commitConfirmedImport(candidate: Candidate) {
        guard case .folder = candidate.source else {
            return
        }
        runCandidateMutation(candidate: candidate) {
            try await importer.startImport(candidate.key)
        }
    }

    /// Record the answer against the draft the person saw.
    func chooseArtistIdentityConflict(
        candidate: Candidate,
        metadataRevision: UInt64,
        choice: BridgeArtistIdentityConflictChoice
    ) {
        runCandidateMutation(candidate: candidate) {
            try await importer.chooseCandidateArtistIdentity(
                candidate.key,
                metadataRevision: metadataRevision,
                choice: choice
            )
        }
    }

    /// Run one of the pane's commands. Core clears the failure the pane states
    /// as it starts and stores this one's in its place, so a failure comes
    /// back here only when the pane does not state it.
    private func runCandidateMutation(
        candidate: Candidate,
        operation: @escaping @MainActor () async throws -> BridgePaneOutcome
    ) {
        candidateMutationTasks[.candidate(candidate.key)]?.cancel()
        candidateMutationTasks[.candidate(candidate.key)] = Task { @MainActor in
            do {
                _ = try await operation()
            }
            catch is CancellationError {
                importConfirmLogger.debug(
                    "candidate command cancelled for \(bridgeCandidateKeyText(candidateKey: candidate.key))"
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
