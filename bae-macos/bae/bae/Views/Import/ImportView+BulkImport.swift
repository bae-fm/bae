import BaeKit
import SwiftUI

// MARK: - Bulk import (importable Pending rows)

extension ImportView {
    /// Import every Ready candidate named in `keys` — the foot bar's action.
    /// Each is one existing single import, dispatched independently; there is
    /// no batch primitive in core. A bulk import never opens the mapping
    /// pane: it commits straight onto the row's matched release with no
    /// metadata edit, which is exactly what the Ready rule already guarantees
    /// is safe (one confident match, not in the library, counts and lengths
    /// agree). If the candidate has a selected cover, the same request shape as
    /// an individual commit carries it.
    func importReadyCandidates(_ keys: [String]) {
        guard !keys.isEmpty else {
            return
        }
        let storageMode = configStore.config.importStorageMode(
            cloud: storageCloud
        )
        // Starting an import suspends — core claims the candidate before it
        // queues the command — so the run happens off the foot bar's action.
        Task {
            var failureCount = 0
            let requested = Set(keys)
            // Selection can outlive the row that earned it (imported by a
            // faster sibling call, or reclassified), so the Ready set core
            // published is what decides which keys still commit.
            for ready in importStore.summary.ready
            where requested.contains(ready.candidateKey) {
                do {
                    try await importer.startImport(
                        ImportCommitRequest(
                            candidateKey: ready.candidateKey,
                            selectedCover:
                                importStore
                                .candidate(forKey: ready.candidateKey)?
                                .selectedCover?
                                .selection,
                            storageMode: storageMode,
                            pin: storagePinned,
                            // The row's stored decision, in the shape commit
                            // takes — identification settled this candidate on
                            // one match and recorded the pick, so a bulk import
                            // commits the same claim opening the pane would
                            // state.
                            identityChoice: ready.claim,
                            userEdit: nil
                        )
                    )
                }
                catch {
                    failureCount += 1
                }
            }
            uiStore.clearReadySelection()
            if failureCount > 0 {
                uiStore.showError(
                    String(localized: "\(failureCount) imports couldn't start")
                )
            }
        }
    }
}
