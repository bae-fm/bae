import BaeKit
import SwiftUI

/// One line summarizing the download queue: a "Paused" chip when the user has
/// paused it, otherwise the count summary (downloading / failed / queued).
/// `compact` shrinks it to caption size for the library strip.
struct DownloadQueueSummaryLine: View {
    let snapshot: BridgeDownloadSnapshot
    let compact: Bool

    var body: some View {
        Group {
            if snapshot.paused {
                Label("Paused", systemImage: "pause.circle.fill")
                    .foregroundStyle(.orange)
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .foregroundStyle(.secondary)
            }
        }
        .font(compact ? .caption : nil)
    }
}

#if DEBUG
#Preview {
    DownloadQueueSummaryLine(
        snapshot: PreviewData.downloadSnapshot(queued: 2, active: 1),
        compact: false
    )
}
#endif
