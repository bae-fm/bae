import BaeKit
import SwiftUI

// MARK: - Bulk import (importable Pending rows)

extension ImportView {
    /// Import every Ready candidate named in `keys` — the foot bar's action.
    /// Each is one existing single import, dispatched independently; there is
    /// no batch primitive in core. A bulk import never opens the mapping pane
    /// and needs nothing from it: the pick, the metadata and the cover are
    /// stored under each candidate, so committing without opening it writes
    /// exactly what opening it would have shown.
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
                            storageMode: storageMode,
                            pin: storagePinned,
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
