import BaeKit
import SwiftUI

/// The Sync section's banner for the sync error and the operations left
/// waiting; it reads `syncStatusStore` itself so only it re-renders.
struct SyncErrorBanner: View {
    @Environment(SyncStatusStore.self)
    var syncStatusStore
    let onReconnect: () async -> Void
    let onRetryBlocked: (String) async throws -> Void

    @State
    private var reconnecting = false

    // Nothing when healthy: an empty container would still take row spacing.
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
                    .foregroundStyle(Theme.warning)
                Text("Sync is failing")
                    .themeText(.strong)
            }
            ErrorDetailDisclosure(
                error: syncError,
                tint: .secondary,
                showIcon: false
            )
            if syncStatusStore.canReconnect {
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
    }

    /// The operations sync stopped on, each with its own Retry, since later
    /// cycles skip them.
    private var blockedOperations: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(Theme.warning)
                Text("Sync is waiting on you")
                    .themeText(.strong)
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

/// One stopped operation: its kind, what it was, why it stopped, and Retry.
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
                .themeText(.rowTitle)
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

    /// A refused retry shows its error here; one that works drops the row.
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
                    canReconnect: true,
                    blocked: [],
                    lastSyncTime: nil,
                    syncing: false,
                    syncReady: false,
                    indicator: .error
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
                    canReconnect: false,
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
                    syncReady: true,
                    indicator: .error
                )
            )
        )
    }
#endif
