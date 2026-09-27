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

    /// Cancel the candidate's import from the pane — the same cancel the
    /// row's menu runs. An import that has begun writing its release is not
    /// cancelled and completes; the bar stops offering the cancel by then,
    /// and a cancel that crossed that moment says so on the banner.
    func cancelImport(candidate: Candidate) {
        runCandidateMutation(candidate: candidate) {
            try await importer.cancelImport(candidate.key)
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

    /// Run one of the pane's commands. Core clears the failure the pane states
    /// as it starts and stores this one's in its place, so a failure comes
    /// back here only when the pane does not state it.
    private func runCandidateMutation(
        candidate: Candidate,
        operation: @escaping @MainActor () async throws -> BridgePaneOutcome
    ) {
        candidateMutationTasks[candidate.key]?.cancel()
        candidateMutationTasks[candidate.key] = Task { @MainActor in
            do {
                _ = try await operation()
            }
            catch is CancellationError {
                importConfirmLogger.debug(
                    "candidate command cancelled for \(candidate.key)"
                )
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
