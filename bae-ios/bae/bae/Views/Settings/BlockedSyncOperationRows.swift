import BaeKit
import SwiftUI

/// Sync operations that failed in a way running them again cannot fix, so sync
/// skips them until someone presses Retry.
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

/// One blocked operation: its kind, which operation, why it stopped, and Retry.
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
            // coven's untranslated reason, for a person to act on or paste into
            // a report.
            Text(operation.error)
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            if let retryError {
                Text(retryError)
                    .font(.caption)
                    .foregroundStyle(Theme.danger)
            }
            if retrying {
                ProgressView()
            }
            else {
                Button("Retry") { run() }
            }
        }
    }

    /// A refused retry shows its reason here instead of leaving the button
    /// looking inert.
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
