import BaeKit
import SwiftUI

extension ImportView {
    func importReadyCandidates() {
        let candidates = ImportCandidateSelection(
            importStore: importStore,
            uiStore: uiStore
        )
        .candidates(for: .importReady)
        performCandidateAction(
            ImportCandidateActionOffer(
                action: .importReady,
                candidates: candidates
            )
        )
    }

    func performCandidateAction(_ offer: ImportCandidateActionOffer) {
        let storageMode = configStore.config.importStorageMode(
            cloud: storageCloud
        )
        let pin = storagePinned
        uiStore.candidateActionRun.start(
            action: offer.action,
            candidates: offer.candidates,
            uiStore: uiStore,
            before: commitAndEndEditing
        ) { key in
            switch offer.action {
            case .importReady:
                try await importer.startImport(
                    ImportCommitRequest(
                        candidateKey: key,
                        storageMode: storageMode,
                        pin: pin
                    )
                )
            // Re-asking what failed is the same command as identifying
            // again: the run reads the candidate's inputs afresh, and the
            // response cache answers the lookups that had succeeded. The two
            // actions differ in what the row offers, not in what core does.
            case .identify, .retryIdentification:
                importer.rerunIdentifyForCandidate(key)
            case .resetToTags:
                _ = try await importer.applyCandidateFileTags(key)
            case .clearMetadata:
                _ = try await importer.clearCandidateMetadata(key)
            case .skip:
                try await importer.setCandidateSkipped(key, true)
            case .restore:
                try await importer.setCandidateSkipped(key, false)
            }
        }
    }
}
