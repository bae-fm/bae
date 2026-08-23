import BaeKit
import SwiftUI

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
        // Start each attempt from a clean error state so a prior failed
        // commit's banner doesn't linger over a now-succeeding retry.
        importStore.mutateCandidate(forKey: candidate.key) { $0.error = nil }
        let request = ImportCommitRequest(
            candidateKey: candidate.key,
            storageMode: configStore.config.importStorageMode(
                cloud: storageCloud
            ),
            pin: storagePinned,
        )
        Task { @MainActor in
            do {
                try await importer.startImport(request)
            }
            catch is CancellationError {}
            catch {
                importStore.mutateCandidate(forKey: candidate.key) {
                    $0.error = error.displayLine
                }
            }
        }
    }
}
