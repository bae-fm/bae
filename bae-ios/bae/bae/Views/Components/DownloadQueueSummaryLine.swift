import BaeKit
import SwiftUI

/// One line summarizing the download queue: a Paused chip, or its counts.
struct DownloadQueueSummaryLine: View {
    let snapshot: BridgeDownloadSnapshot

    var body: some View {
        Group {
            if snapshot.paused {
                StatusChip(
                    "Paused",
                    tone: .activity,
                    symbol: "pause.circle.fill"
                )
            }
            else if !snapshot.summaryText.isEmpty {
                Text(snapshot.summaryText)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

#if DEBUG
#Preview {
    DownloadQueueSummaryLine(
        snapshot: PreviewData.downloadSnapshot(queued: 2, active: 1)
    )
}
#endif
