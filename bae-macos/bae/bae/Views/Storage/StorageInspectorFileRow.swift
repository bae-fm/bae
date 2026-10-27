import BaeKit
import SwiftUI

/// One file in the storage inspector: a release file with its upload while
/// one waits.
struct StorageInspectorFileRow: View {
    let row: BridgeStorageInspectorFile
    /// How fast the row's upload goes; `nil` while no rate has arrived for
    /// it, or when the row has no upload.
    let throughputBps: UInt64?

    /// The lines under the name start under it, past the icon column.
    private static let detailIndent = ThemeIcon.medium.size + ThemeSpace.related

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(spacing: ThemeSpace.related) {
                Image(systemName: "doc")
                    .themeIcon(.medium)
                    .foregroundStyle(.secondary)
                    .frame(width: ThemeIcon.medium.size)
                Text(row.file.originalFilename)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(row.file.originalFilename)
                Spacer(minLength: ThemeSpace.related)
                Text(row.file.fileSizeText)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Text(row.file.audioFormat?.text ?? "")
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .padding(.leading, Self.detailIndent)
            if let upload = row.upload {
                UploadOperationLine(
                    upload: upload,
                    throughputBps: throughputBps
                )
                .padding(.leading, Self.detailIndent)
            }
        }
        .padding(.vertical, ThemeSpace.line)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("file:\(row.file.id)")
    }
}

/// One upload's state, its rate while bytes move, and its progress bar. A
/// pending upload shows why coven is waiting instead of its state.
private struct UploadOperationLine: View {
    let upload: BridgeFileUpload
    /// How fast the file goes; `nil` while no rate has arrived for it.
    let throughputBps: UInt64?

    var body: some View {
        let bar = upload.bar
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            HStack(spacing: ThemeSpace.related) {
                StatusChip(
                    verbatim: status,
                    tone: tone,
                    symbol: symbol
                )
                .help(status)
                Text(bar?.text ?? "")
                    .monospacedDigit()
                Spacer(minLength: 0)
                Text(throughputText)
                    .monospacedDigit()
                    .fixedSize()
            }
            .themeText(.detail)
            .foregroundStyle(.secondary)
            ProgressTrackBar(progress: bar?.fraction ?? 0)
                .opacity(bar == nil ? 0 : 1)
                .accessibilityHidden(bar == nil)
        }
    }

    private var status: String {
        if let pending = upload.pending {
            return localizedCoreString(pending.reason.key)
        }
        return localizedCoreString(
            bridgeUploadFileStateKey(state: upload.state)
        )
    }

    private var symbol: String {
        if upload.pending != nil { return "exclamationmark.triangle.fill" }
        return switch upload.state {
        case .waiting: "clock"
        case .uploading: "arrow.up.circle.fill"
        }
    }

    private var tone: StatusTone {
        if upload.pending != nil { return .warning }
        return switch upload.state {
        case .waiting: .neutral
        case .uploading: .activity
        }
    }

    private var throughputText: String {
        guard let throughputBps, throughputBps > 0 else { return "" }
        return QueueSummary.throughputText(bytesPerSecond: throughputBps)
    }
}
