import BaeKit
import SwiftUI

extension ImportView {
    /// Run one action a selection offers — from the pane a selection opens
    /// or from the list's menu, which offer the same actions. One that
    /// replaces what a person may have chosen is asked about first.
    func requestCandidateAction(_ offer: ImportCandidateActionOffer) {
        guard offer.enabled else { return }
        if offer.action.needsConfirmation {
            candidateActionConfirmation = offer
            return
        }
        performCandidateAction(offer)
    }

    func performCandidateAction(_ offer: ImportCandidateActionOffer) {
        switch offer.action {
        // One action over the whole selection rather than one per folder.
        case .combine:
            combineCandidates(offer.keys)
            return
        // Nothing to write and nothing to report: each folder is shown.
        case .revealFolder:
            offer.keys.forEach(revealCandidateSources)
            return
        case .import, .identify, .cancelIdentification, .cancelImport,
            .retryIdentification, .resetToFileMetadata, .clearMetadata,
            .separate, .skip, .restore:
            break
        }
        uiStore.candidateActionRun.start(
            action: offer.action,
            targets: offer.targets,
            uiStore: uiStore,
            before: commitAndEndEditing
        ) { key in
            try await runCandidateAction(offer.action, on: key)
        }
    }

    /// One folder's part of a run.
    private func runCandidateAction(
        _ action: BridgeCandidateAction,
        on key: String
    ) async throws {
        switch action {
        // A row being identified or already importing when the run
        // reaches it is refused by core, and the refusal joins the run's
        // report beside its name.
        case .import:
            try await importer.importSelected(key)
        // Re-asking what failed is the same command as identifying
        // again: the run reads the candidate's inputs afresh, and the
        // response cache answers the lookups that had succeeded. The two
        // actions differ in what the row offers, not in what core does.
        case .identify, .retryIdentification:
            importer.rerunIdentifyForCandidate(key)
        case .cancelIdentification:
            try await importer.cancelIdentification([key])
        case .cancelImport:
            try await importer.cancelImport(key)
        case .resetToFileMetadata:
            _ = try await importer.applyCandidateFileMetadata(key)
        case .clearMetadata:
            _ = try await importer.clearCandidateMetadata(key)
        case .separate:
            try await importer.separateCandidate(key)
        case .skip:
            try await importer.setCandidateSkipped(key, true)
        case .restore:
            try await importer.setCandidateSkipped(key, false)
        case .combine, .revealFolder:
            assertionFailure(
                "\(action) does not run folder by folder"
            )
        }
    }
}
