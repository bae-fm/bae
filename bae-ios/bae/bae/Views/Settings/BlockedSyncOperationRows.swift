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
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Label {
                    Text("Sync is waiting on you")
                        .themeText(.strong)
                } icon: {
                    if let symbol = StatusTone.warning.symbol {
                        Image(systemName: symbol)
                            .foregroundStyle(StatusTone.warning.color)
                    }
                }
                ForEach(syncStatusStore.blocked, id: \.id) { operation in
                    BlockedSyncOperationRow(operation: operation, retry: retry)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .notice(.warning)
            // Each Retry takes only its own taps, not the whole list row's.
            .buttonStyle(.borderless)
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
    private var retryError: DisplayError?

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            Text(operation.kind.localizedName)
                .themeText(.rowTitle)
            ErrorDetailDisclosure(
                error: DisplayError(
                    line: operation.description,
                    detail: operation.error
                ),
                tone: .neutral,
                showIcon: false
            )
            if let retryError {
                ErrorDetailDisclosure(error: retryError)
            }
            if retrying {
                ProgressView()
            }
            else {
                Button("Retry") { run() }
            }
        }
    }

    /// A refused retry shows its error here; one that works drops the row.
    private func run() {
        retryError = nil
        retrying = true
        let id = operation.id
        Task {
            do {
                try await retry(id)
            }
            catch {
                retryError = DisplayError(error)
            }
            retrying = false
        }
    }
}
