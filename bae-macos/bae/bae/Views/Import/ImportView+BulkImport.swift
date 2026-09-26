import BaeKit
import SwiftUI

extension ImportView {
    /// Stop what is running for one candidate, from its row's menu. A row
    /// offers only the cancel actions its live state carries.
    func cancelCandidateWork(_ key: String, _ action: BridgeCandidateAction) {
        switch action {
        case .cancelIdentification:
            importer.cancelIdentification([key])
        case .cancelImport:
            Task {
                do { try await importer.cancelImport(key) }
                catch { uiStore.showError(error) }
            }
        case .importReady, .identify, .retryIdentification,
            .resetToFileMetadata, .clearMetadata, .skip, .restore:
            assertionFailure("\(action) is not a cancel action")
        }
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
            // The Ready set is what the tables say; a row being identified
            // or already importing when the run reaches it is refused by
            // core, and the refusal joins the run's report beside its name.
            case .importReady:
                try await importer.importReady(
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
            case .cancelIdentification:
                importer.cancelIdentification([key])
            case .cancelImport:
                try await importer.cancelImport(key)
            case .resetToFileMetadata:
                _ = try await importer.applyCandidateFileMetadata(key)
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
