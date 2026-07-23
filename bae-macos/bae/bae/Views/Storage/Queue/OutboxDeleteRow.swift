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
