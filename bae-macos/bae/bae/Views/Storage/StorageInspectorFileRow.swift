import BaeKit
import SwiftUI

struct StorageInspectorFileRow: View {
    let row: BridgeStorageInspectorFile

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: row.uploadSymbol)
                    .foregroundStyle(row.uploadTint)
                    .frame(width: 16)
                    .help(row.uploadStatus)
                    .accessibilityLabel(row.uploadStatus)
                Text(row.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(row.name)
                Spacer(minLength: 8)
                Text(row.sizeText)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            HStack(spacing: 8) {
                Text(row.file?.audioFormat?.text ?? "")
                    .lineLimit(1)
                Spacer(minLength: 0)
                Text(row.progressText)
                    .foregroundStyle(row.uploadTint)
                    .lineLimit(1)
                    .help(row.progressText)
                Text(row.throughputText)
                    .monospacedDigit()
                    .fixedSize()
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.leading, 24)
            ProgressTrackBar(progress: row.upload?.bar?.fraction ?? 0)
                .opacity(row.upload?.bar == nil ? 0 : 1)
                .accessibilityHidden(row.upload?.bar == nil)
                .padding(.leading, 24)
        }
        .padding(.vertical, 3)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(row.identity)
    }
}

extension BridgeStorageInspectorFile {
    var identity: String {
        switch self {
        case .releaseFile(let file, _): "file:\(file.id)"
        case .upload(let upload): "upload:\(upload.fileId)"
        }
    }

    var file: BridgeFile? {
        switch self {
        case .releaseFile(let file, _): file
        case .upload: nil
        }
    }

    var upload: BridgeUploadFileOp? {
        switch self {
        case .releaseFile(_, let upload): upload
        case .upload(let upload): upload
        }
    }

    var name: String {
        switch self {
        case .releaseFile(let file, _): file.originalFilename
        case .upload(let upload):
            switch upload.label {
            case .filename(let name): name
            case .cover: QueueSummary.message("core.import.role.cover")
            case .artistImage:
                QueueSummary.message("core.outbox.file.artist_image")
            case .unwinding: QueueSummary.message("core.outbox.file.unwinding")
            }
        }
    }

    var sizeText: String {
        switch self {
        case .releaseFile(let file, _): file.fileSizeText
        case .upload(let upload):
            Int64(upload.sourceBytesTotal).formatted(.byteCount(style: .file))
        }
    }

    var uploadSymbol: String {
        switch upload?.state {
        case .queued: "clock"
        case .preparing: "seal"
        case .prepared: "checkmark.circle"
        case .uploading: "arrow.up.circle.fill"
        case .retrying: "exclamationmark.triangle.fill"
        case .uploaded: "icloud"
        case nil: "doc"
        }
    }

    var uploadTint: Color {
        switch upload?.state {
        case .preparing, .uploading: .orange
        case .retrying: .red
        case .uploaded: .blue
        case .queued, .prepared, nil: .secondary
        }
    }

    var uploadStatus: String {
        switch upload?.state {
        case .queued: String(localized: "Queued")
        case .preparing: QueueSummary.countLabel("core.outbox.preparing", 1)
        case .prepared: QueueSummary.countLabel("core.outbox.prepared", 1)
        case .uploading: QueueSummary.countLabel("core.queue.uploading", 1)
        case .retrying: QueueSummary.countLabel("core.outbox.retrying", 1)
        case .uploaded: QueueSummary.countLabel("core.outbox.uploaded", 1)
        case nil: name
        }
    }

    var throughputText: String {
        guard let upload, upload.throughputBps > 0 else { return "" }
        return QueueSummary.throughputText(bytesPerSecond: upload.throughputBps)
    }

    var progressText: String {
        guard let upload else { return "" }
        if upload.state == .retrying {
            guard let error = upload.lastError else {
                preconditionFailure("a retrying upload file has no error")
            }
            return error
        }
        return upload.bar?.text ?? ""
    }
}
