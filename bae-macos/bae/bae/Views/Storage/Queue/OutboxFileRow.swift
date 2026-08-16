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

            Text(displayName)
                .font(.caption)
                .lineLimit(1)
                .foregroundStyle(.secondary)

            Spacer()

            ProgressTrackBar(progress: fraction)
                .frame(width: 140)
                .opacity(showsProgress ? 1 : 0)

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

    private var displayName: String {
        switch file.label {
        case .filename(let name):
            name
        case .cover:
            QueueSummary.message("core.import.role.cover")
        case .artistImage:
            QueueSummary.message("core.outbox.file.artist_image")
        }
    }

    @ViewBuilder
    private var stateIcon: some View {
        switch file.state {
        case .queued:
            Image(systemName: "clock")
                .foregroundStyle(.secondary)
        case .preparing:
            Image(systemName: "seal")
                .foregroundStyle(.orange)
        case .prepared:
            Image(systemName: "checkmark.circle")
                .foregroundStyle(.secondary)
        case .uploading:
            Image(systemName: "arrow.up.circle.fill")
                .foregroundStyle(.orange)
        case .retrying:
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
                .help(retryError)
        case .uploaded:
            Image(systemName: "icloud")
                .foregroundStyle(.blue)
        }
    }

    private var retryError: String {
        guard let error = file.lastError else {
            preconditionFailure("a retrying upload file has no error")
        }
        return error
    }

    private var fraction: Double {
        guard file.progressBytesTotal > 0 else { return 0 }
        precondition(
            file.bytesDone <= file.progressBytesTotal,
            "upload file progress cannot exceed its exact total"
        )
        return Double(file.bytesDone) / Double(file.progressBytesTotal)
    }

    private var showsProgress: Bool {
        (file.state == .preparing || file.state == .uploading)
            && file.progressBytesTotal > 0
    }

    /// "6.2 MB of 12.4 MB" while transferring; just the size otherwise.
    private var bytesText: String {
        let sourceSize = Int64(file.sourceBytesTotal)
            .formatted(.byteCount(style: .file))
        guard file.state == .preparing || file.state == .uploading,
            file.progressBytesTotal > 0
        else { return sourceSize }
        let total = Int64(file.progressBytesTotal)
            .formatted(.byteCount(style: .file))
        return String(
            format: QueueSummary.message("core.outbox.bytes_progress"),
            Int64(file.bytesDone).formatted(.byteCount(style: .file)),
            total
        )
    }
}

#if DEBUG
    #Preview("File states") {
        VStack(spacing: 0) {
            ForEach(PreviewData.uploadFileOps, id: \.fileId) { file in
                OutboxFileRow(file: file)
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
