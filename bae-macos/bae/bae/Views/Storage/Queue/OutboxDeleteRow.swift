import BaeKit
import SwiftUI

/// One queued cloud-delete row: the removed blob and a pending-delete badge.
///
/// No cancel button: the object is already in the cloud and its row is gone, so
/// abandoning the removal would strand the object with nothing left to name it.
struct OutboxDeleteRow: View {
    let op: BridgeDeleteOp

    var body: some View {
        QueueRow(
            icon: "trash",
            createdAt: op.createdAt
        ) {
            Text("\(op.namespace)/\(op.blobId)")
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
            ForEach(PreviewData.deleteOps, id: \.blobId) { op in
                OutboxDeleteRow(op: op)
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
