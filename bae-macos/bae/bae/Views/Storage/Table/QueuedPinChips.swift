import BaeKit
import SwiftUI

/// What a pin waiting in the download queue shows: that it is queued, how many
/// entries are ahead of it, and what holds the queue back.
struct QueuedPinChips: View {
    let ahead: UInt32
    let wait: BridgeQueueWait?

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            StatusChip("Queued", tone: .activity)
            if ahead > 0 {
                StatusChip("\(Int(ahead)) ahead")
            }
            if let wait {
                QueueWaitChip(wait: wait)
            }
        }
    }
}

/// What keeps the download queue from taking its next entry.
struct QueueWaitChip: View {
    let wait: BridgeQueueWait

    var body: some View {
        switch wait {
        case .paused:
            StatusChip("Paused", tone: .warning)
        case .offline:
            StatusChip("Waiting for network", tone: .warning)
        }
    }
}

#if DEBUG
    #Preview("Queued pins") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            QueuedPinChips(ahead: 0, wait: nil)
            QueuedPinChips(ahead: 2, wait: nil)
            QueuedPinChips(ahead: 2, wait: .paused)
            QueuedPinChips(ahead: 0, wait: .offline)
        }
        .padding()
    }
#endif
