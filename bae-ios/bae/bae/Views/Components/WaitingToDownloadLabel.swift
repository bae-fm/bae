import BaeKit
import SwiftUI

/// The status of a release queued for download that hasn't started yet.
struct WaitingToDownloadLabel: View {
    var body: some View {
        Label("Waiting to download", systemImage: "clock")
            .themeText(.detail)
            .foregroundStyle(.secondary)
    }
}

#if DEBUG
#Preview {
    WaitingToDownloadLabel()
}
#endif
