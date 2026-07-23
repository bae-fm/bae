import BaeKit
import Foundation
import SwiftUI

/// One file inside an expanded release row: state icon, name, and — while the
/// file transfers — a live determinate bar with its byte progress. Completed
/// files read as a checkmark with their size; a failed file carries its error
/// as a tooltip and waits for the next retry.
struct OutboxFileRow: View {
    let file: BridgeUploadFileOp

    var body: some View {
        HStack(spacing: 12) {
            stateIcon
                .frame(width: 16)

            Text(file.displayName)
                .font(.caption)
                .lineLimit(1)
                .foregroundStyle(.secondary)

            Spacer()

            if file.state == .uploading {
                ProgressView(value: fraction)
                    .progressViewStyle(.linear)
                    .frame(width: 140)
            }

            Text(bytesText)
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.tertiary)
                .frame(width: 130, alignment: .leading)
        }
        .padding(.leading, 44)
        .padding(.trailing)
        .padding(.vertical, 3)
    }

    @ViewBuilder
    private var stateIcon: some View {
        switch file.state {
        case .queued:
            Image(systemName: "clock")
                .foregroundStyle(.secondary)
        case .uploading:
            Image(systemName: "arrow.up.circle.fill")
                .foregroundStyle(.orange)
        case .retrying:
            // The converter sets `lastError` for every retrying file; the
            // conditional only spares a hypothetical empty tooltip.
            let icon = Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
            if let error = file.lastError {
                icon.help(error)
            }
            else {
                icon
            }
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
        }
    }

    private var fraction: Double {
        guard file.bytesTotal > 0 else { return 0 }
        return Double(file.bytesDone) / Double(file.bytesTotal)
    }

    /// "6.2 MB of 12.4 MB" while transferring; just the size otherwise.
    private var bytesText: String {
        let total = Int64(file.bytesTotal).formatted(.byteCount(style: .file))
        guard file.state == .uploading else { return total }
        return String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(file.bytesDone).formatted(.byteCount(style: .file)),
            total
        )
    }
}
