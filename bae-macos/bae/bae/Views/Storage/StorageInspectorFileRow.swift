import BaeKit
import SwiftUI

/// One file in the storage inspector: a release file with the uploads coven
/// holds for it, or an upload that belongs to no release file.
struct StorageInspectorFileRow: View {
    let row: BridgeStorageInspectorFile

    /// The lines under the name start under it, past the icon column.
    private static let detailIndent = ThemeIcon.medium.size + ThemeSpace.related

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(spacing: ThemeSpace.related) {
                Image(systemName: row.symbol)
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
                    .frame(width: ThemeIcon.medium.size)
                Text(row.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(row.name)
                Spacer(minLength: ThemeSpace.related)
                Text(row.file?.fileSizeText ?? "")
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Text(row.file?.audioFormat?.text ?? "")
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .padding(.leading, Self.detailIndent)
            ForEach(row.operations, id: \.fileId) { operation in
                UploadOperationLine(operation: operation)
                    .padding(.leading, Self.detailIndent)
            }
        }
        .padding(.vertical, ThemeSpace.line)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(row.identity)
    }
}

/// One upload's state, its rate while bytes move, and its progress bar. A
/// pending upload shows why coven is waiting instead of its state.
private struct UploadOperationLine: View {
    let operation: BridgeUploadFileOp

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(spacing: ThemeSpace.related) {
                StatusChip(
                    verbatim: status,
                    tone: tone,
                    symbol: symbol
                )
                .help(status)
                Text(operation.bar?.text ?? "")
                    .monospacedDigit()
                Spacer(minLength: 0)
                Text(throughputText)
                    .monospacedDigit()
                    .fixedSize()
            }
            .themeText(.detail)
            .foregroundStyle(.secondary)
            ProgressTrackBar(progress: operation.bar?.fraction ?? 0)
                .opacity(operation.bar == nil ? 0 : 1)
                .accessibilityHidden(operation.bar == nil)
        }
    }

    private var status: String {
        if let pending = operation.pending {
            return localizedCoreString(pending.reason.key)
        }
        return localizedCoreString(
            bridgeUploadFileStateKey(state: operation.state)
        )
    }

    private var symbol: String {
        if operation.pending != nil { return "exclamationmark.triangle.fill" }
        return switch operation.state {
        case .waiting: "clock"
        case .uploading: "arrow.up.circle.fill"
        }
    }

    private var tone: StatusTone {
        if operation.pending != nil { return .warning }
        return switch operation.state {
        case .waiting: .neutral
        case .uploading: .activity
        }
    }

    private var throughputText: String {
        guard operation.throughputBps > 0 else { return "" }
        return QueueSummary.throughputText(
            bytesPerSecond: operation.throughputBps
        )
    }
}

extension BridgeStorageInspectorFile {
    var identity: String {
        switch self {
        case .releaseFile(let file, _): "file:\(file.id)"
        case .upload(let operation): "upload:\(operation.fileId)"
        }
    }

    var file: BridgeFile? {
        switch self {
        case .releaseFile(let file, _): file
        case .upload: nil
        }
    }

    var operations: [BridgeUploadFileOp] {
        switch self {
        case .releaseFile(_, let operations): operations
        case .upload(let operation): [operation]
        }
    }

    var symbol: String {
        switch self {
        case .releaseFile: "doc"
        case .upload(let operation):
            switch operation.label {
            case .filename: "doc"
            case .cover: "photo"
            case .artistImage: "person.crop.square"
            case .avatar: "person.crop.circle"
            case .deletedReleaseFile: "trash"
            }
        }
    }

    var name: String {
        switch self {
        case .releaseFile(let file, _): return file.originalFilename
        case .upload(let operation):
            if case .filename(let name) = operation.label { return name }
            guard let key = bridgeUploadFileLabelKey(label: operation.label)
            else {
                preconditionFailure("only a file name has no catalog label")
            }
            return localizedCoreString(key)
        }
    }
}
