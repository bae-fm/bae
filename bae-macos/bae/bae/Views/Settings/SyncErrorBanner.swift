import BaeKit
import SwiftUI

/// Banner at the top of the Library settings Sync section when the sync loop
/// has reported an error. Reads `configStore.syncError` directly so only this
/// view re-renders on sync-health transitions, not the whole settings tab.
struct SyncErrorBanner: View {
    @Environment(ConfigStore.self)
    var configStore
    let onReconnect: () -> Void

    var body: some View {
        if let syncError = configStore.syncError {
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
