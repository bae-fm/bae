import SwiftUI

/// The "waiting in the download queue" status, shown on a queued row and on the
/// album-detail control for a release that hasn't started downloading yet.
struct WaitingToDownloadLabel: View {
    var body: some View {
        Label("Waiting to download", systemImage: "clock")
            .font(.caption)
            .foregroundStyle(.secondary)
    }
}
