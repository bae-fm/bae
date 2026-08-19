import BaeKit
import SwiftUI

/// Banner at the top of the Library settings Sync section when the sync loop
/// has reported an error. Reads `syncStatusStore.error` directly so only this
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

    @State
    private var reconnecting = false

    var body: some View {
        if let syncError = syncStatusStore.error {
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
    }
}

#if DEBUG
    #Preview("Sync failing") {
        Form {
            Section("Sync") {
                SyncErrorBanner(onReconnect: {})
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
                    lastSyncTime: nil,
                    syncing: false,
                    syncReady: false
                )
            )
        )
    }
#endif
