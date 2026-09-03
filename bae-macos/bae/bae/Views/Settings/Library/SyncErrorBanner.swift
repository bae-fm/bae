import BaeKit
import SwiftUI

/// Banner at the top of the Library settings Sync section for everything about
/// sync that needs a person: the loop's own error, and the durable operations a
/// completed cycle left waiting. Reads `syncStatusStore` directly so only this
/// view re-renders on sync-health transitions, not the whole settings tab.
///
/// Reconnect retries the connection the library is already configured for, and
/// the retry runs over the network, so the button stays busy until it settles.
/// It settles visibly either way: a retry that took clears the error and takes
/// this banner with it, and one that didn't leaves the banner showing the new
/// reason.
struct SyncErrorBanner: View {
    @Environment(SyncStatusStore.self)
    var syncStatusStore
    let onReconnect: () async -> Void
    let onRetryBlocked: (String) async throws -> Void

    @State
    private var reconnecting = false

    // Nothing at all when sync is healthy — an empty container would still take
    // the enclosing Form section's row spacing.
    var body: some View {
        if syncStatusStore.error != nil || !syncStatusStore.blocked.isEmpty {
            VStack(alignment: .leading, spacing: 14) {
                if let syncError = syncStatusStore.error {
                    failingCycle(syncError)
                }
                if !syncStatusStore.blocked.isEmpty {
                    blockedOperations
                }
            }
        }
    }

    private func failingCycle(_ syncError: DisplayError) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text("Sync is failing")
                    .font(.callout)
                    .bold()
            }
            ErrorDetailDisclosure(
                error: syncError,
                tint: .secondary,
                showIcon: false
            )
            HStack(spacing: 8) {
                Button("Reconnect") {
                    Task {
                        reconnecting = true
                        await onReconnect()
                        reconnecting = false
                    }
                }
                .disabled(reconnecting)
                if reconnecting {
                    ProgressView()
                        .controlSize(.small)
                }
            }
        }
    }

    /// Every operation the last completed cycle left stopped. Each waits on a
    /// person indefinitely — later cycles skip it — so each carries its own
    /// Retry; a row leaves the list when the next status no longer names it.
    private var blockedOperations: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                Text("Sync is waiting on you")
                    .font(.callout)
                    .bold()
            }
            ForEach(syncStatusStore.blocked, id: \.id) { operation in
                BlockedSyncOperationRow(
                    operation: operation,
                    onRetry: onRetryBlocked
                )
            }
        }
    }
}

/// One stopped operation: what kind of work it was, which operation, why it
/// stopped, and the button that hands it back to the sync loop.
private struct BlockedSyncOperationRow: View {
    let operation: BridgeBlockedSyncOperation
    let onRetry: (String) async throws -> Void

    @State
    private var retrying = false
    @State
    private var retryError: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(operation.kind.localizedName)
                .font(.callout)
            ErrorDetailDisclosure(
                error: DisplayError(
                    line: operation.description,
                    detail: operation.error
                ),
                tint: .secondary,
                showIcon: false
            )
            if let retryError {
                ErrorDetailDisclosure(error: retryError)
            }
            HStack(spacing: 8) {
                Button("Retry") { retry() }
                    .disabled(retrying)
                if retrying {
                    ProgressView()
                        .controlSize(.small)
                }
            }
        }
    }

    /// A retry that takes drops this row on the next status; one refused —
    /// because the operation is no longer blocked, or the loop is not running —
    /// says so here rather than leaving the button looking inert.
    private func retry() {
        retryError = nil
        retrying = true
        let id = operation.id
        Task {
            do {
                try await onRetry(id)
            }
            catch {
                retryError = DisplayError(error)
            }
            retrying = false
        }
    }
}

#if DEBUG
    #Preview("Sync failing") {
        Form {
            Section("Sync") {
                SyncErrorBanner(onReconnect: {}, onRetryBlocked: { _ in })
            }
        }
        .formStyle(.grouped)
        .frame(width: 500)
        .environment(
            SyncStatusStore(
                snapshot: BridgeSyncStatusSnapshot(
                    error: .Diagnostic(
                        category: .network,
                        detail: "The cloud provider rejected the request."
                    ),
                    blocked: [],
                    lastSyncTime: nil,
                    syncing: false,
                    syncReady: false
                )
            )
        )
    }

    #Preview("Sync waiting on a decision") {
        Form {
            Section("Sync") {
                SyncErrorBanner(onReconnect: {}, onRetryBlocked: { _ in })
            }
        }
        .formStyle(.grouped)
        .frame(width: 500)
        .environment(
            SyncStatusStore(
                snapshot: BridgeSyncStatusSnapshot(
                    error: nil,
                    blocked: [
                        BridgeBlockedSyncOperation(
                            id: "write:write-1",
                            kind: .write,
                            description: "releases/release-3",
                            error: "blob release_files/file-7 is missing"
                        ),
                        BridgeBlockedSyncOperation(
                            id: "reclaim:9f2c",
                            kind: .reclaim,
                            description:
                                "a published batch of library changes",
                            error:
                                "object store-v1/library/packages/12.json: the slot already holds another object"
                        ),
                    ],
                    lastSyncTime: 1_700_000_000_000,
                    syncing: false,
                    syncReady: true
                )
            )
        )
    }
#endif
