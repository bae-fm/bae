import BaeKit
import SwiftUI

/// Banner at the top of the Library settings Sync section when the sync loop
/// has reported an error. Reads `syncStatusStore.error` directly so only this
/// view re-renders on sync-health transitions, not the whole settings tab.
struct SyncErrorBanner: View {
    @Environment(SyncStatusStore.self)
    var syncStatusStore
    let onReconnect: () -> Void

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
                Button("Reconnect") {
                    onReconnect()
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
