import BaeKit
import Foundation
import SwiftUI

/// One file inside an expanded release row: state icon, name, and — while the
/// file transfers — a live determinate bar with the byte progress of the phase
/// it is in. Completed files read as a checkmark with their size; a failed file
/// carries its error as a tooltip and waits for the next retry.
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

            ProgressTrackBar(progress: file.bar?.fraction ?? 0)
                .frame(width: 140)
                .opacity(file.bar == nil ? 0 : 1)

            Text(bytesText)
                .font(.caption)
                .monospacedDigit()
                .lineLimit(1)
                .foregroundStyle(.tertiary)
                // Wide enough for the phase-named form ("Uploading 6.2 MB of
                // 12.4 MB"), which is what a transferring file reads as.
                .frame(width: 190, alignment: .leading)
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
        case .unwinding:
            QueueSummary.message("core.outbox.file.unwinding")
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

    /// "Uploading 6.2 MB of 12.4 MB" while a phase is counting this file's
    /// bytes — the same numbers the bar beside it fills with. A file at rest
    /// has no bar and reads as its own size.
    private var bytesText: String {
        file.bar?.text
            ?? Int64(file.sourceBytesTotal)
            .formatted(.byteCount(style: .file))
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
