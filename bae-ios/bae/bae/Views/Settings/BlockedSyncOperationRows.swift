import BaeKit
import SwiftUI

/// The durable sync operations a completed cycle left waiting on a person, in
/// the Sync section under the status row. Each one failed on a fault that
/// running it again cannot change, so later cycles skip it and it moves only
/// when someone presses Retry; a row leaves the list when the next status no
/// longer names it. Renders nothing while there are none.
struct BlockedSyncOperationRows: View {
    @Environment(SyncStatusStore.self)
    private var syncStatusStore

    let retry: (String) async throws -> Void

    var body: some View {
        if !syncStatusStore.blocked.isEmpty {
            Text("Sync is waiting on you")
                .font(.callout)
                .bold()
        }
        ForEach(syncStatusStore.blocked, id: \.id) { operation in
            BlockedSyncOperationRow(operation: operation, retry: retry)
        }
    }
}

/// One stopped operation: what kind of work it was, which operation, why it
/// stopped, and the button that hands it back to the sync loop.
private struct BlockedSyncOperationRow: View {
    let operation: BridgeBlockedSyncOperation
    let retry: (String) async throws -> Void

    @State
    private var retrying = false
    @State
    private var retryError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(operation.kind.localizedName)
            Text(operation.description)
                .font(.caption)
                .foregroundStyle(.secondary)
            // coven's own reason, untranslated. The kind above names the work
            // in the reader's language; this names what stopped it, which is
            // the part they can act on or paste into a report.
            Text(operation.error)
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            if let retryError {
                Text(retryError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            if retrying {
                ProgressView()
            }
            else {
                Button("Retry") { run() }
            }
        }
    }

    /// A retry that takes drops this row on the next status; one refused —
    /// because the operation is no longer blocked, or the loop is not running —
    /// says so here rather than leaving the button looking inert.
    private func run() {
        retryError = nil
        retrying = true
        let id = operation.id
        Task {
            do {
                try await retry(id)
            }
            catch {
                retryError = error.displayLine
            }
            retrying = false
        }
    }
}
