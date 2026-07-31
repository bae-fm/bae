import BaeKit
import SwiftUI

// MARK: - Bulk import (Ready tab)

extension ImportView {
    /// Import every Ready candidate named in `keys` — the foot bar's action.
    /// Each is one existing single import, dispatched independently; there is
    /// no batch primitive in core. A bulk import never opens the mapping
    /// pane: it commits straight onto the row's matched release with no
    /// cover pick and no metadata edit, which is exactly what the Ready rule
    /// already guarantees is safe (one confident match, not in the library,
    /// counts and lengths agree).
    func importReadyCandidates(_ keys: [String]) {
        guard !keys.isEmpty else {
            return
        }
        let storageMode = configStore.config.importStorageMode(
            managed: storageManaged
        )
        // Starting an import suspends — core claims the candidate before it
        // queues the command — so the run happens off the foot bar's action.
        Task {
            var failureCount = 0
            for key in keys {
                guard
                    let row = importStore.triageRow(forKey: key),
                    let matched = row.matched
                else {
                    // Selection can outlive the row that earned it (imported by
                    // a faster sibling call, or reclassified) — the list content
                    // already intersects the selection against the tab's current
                    // Ready keys, so this is defensive, not expected.
                    continue
                }
                do {
                    try await importer.startImport(
                        key,
                        nil,
                        storageMode,
                        storagePinned,
                        // Core derives this from the evidence that matched: a
                        // disc ID that named one release claims the pressing, a
                        // barcode or a text hit claims only the album. A bulk
                        // import has no claim line to read, so it commits what
                        // the row carries.
                        matched.claim,
                        nil
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
