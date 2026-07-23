import BaeKit
import SwiftUI

/// One queued cloud-delete row: the cloud key, a pending-delete badge, and a
/// cancel button.
struct OutboxDeleteRow: View {
    let op: BridgeDeleteOp
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: "trash",
            createdAt: op.createdAt,
            cancelHelp: "Remove from the sync queue",
            onCancel: onCancel
        ) {
            Text(op.cloudKey)
                .lineLimit(1)
        } badge: {
            Label("Pending delete", systemImage: "clock")
                .foregroundStyle(.secondary)
        }
    }
}

#if DEBUG
    #Preview("Pending delete") {
        VStack(spacing: 0) {
            ForEach(PreviewData.deleteOps, id: \.id) { op in
                OutboxDeleteRow(op: op, onCancel: {})
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
