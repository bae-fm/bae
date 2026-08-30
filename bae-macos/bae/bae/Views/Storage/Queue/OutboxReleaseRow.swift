import BaeKit
import SwiftUI

/// One expandable transfer-inspector row per release (matching the storage
/// table):
/// the release title, its file count and cumulative byte progress, a
/// determinate progress bar, and an aggregate state badge. Expanded (the
/// default), it lists every file with its own state and live progress.
/// Right-click to cancel while core reports that the transition can unwind.
struct OutboxReleaseRow: View {
    let group: BridgeUploadReleaseGroup
    let onCancel: () -> Void

    /// Per-row disclosure, keyed by the `ForEach` identity (the release id).
    /// Expanded by default: the per-file list is the pane's point.
    @State
    private var expanded = true

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 12) {
                    Button {
                        expanded.toggle()
                    } label: {
                        Image(
                            systemName: expanded
                                ? "chevron.down" : "chevron.right"
                        )
                        .foregroundStyle(.secondary)
                        .frame(width: 16)
                    }
                    .buttonStyle(.plain)
                    .help(expanded ? "Hide files" : "Show files")

                    Text(group.displayTitle)
                        .lineLimit(1)
                }

                HStack(spacing: 8) {
                    Text(releaseDetail)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .lineLimit(1)

                    Spacer()

                    UploadActivityLabel(progress: group.progress)
                        .font(.caption)
                        .lineLimit(1)
                }
                .padding(.leading, 28)

                ForEach(group.progress.sourceUnavailablePaths, id: \.self) {
                    path in
                    Text(path)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .help(path)
                        .padding(.leading, 28)
                }

                ProgressTrackBar(progress: group.progress.bar?.fraction ?? 0)
                    .opacity(group.progress.bar == nil ? 0 : 1)
                    .padding(.leading, 28)
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
            .contextMenu {
                if group.progress.canCancel {
                    Button("Cancel", role: .destructive, action: onCancel)
                }
            }
            if expanded {
                ForEach(group.files, id: \.fileId) { file in
                    OutboxFileRow(file: file)
                }
            }
        }
    }

    private var releaseDetail: String {
        let files = String(localized: "\(group.files.count) files")
        let retry =
            group.progress.sourceUnavailablePaths.isEmpty
            ? nil : group.progress.activityText
        return [files, retry, group.progress.bar?.text]
            .compactMap { $0 }
            .joined(separator: " \u{b7} ")
    }
}

#if DEBUG
    #Preview("Uploading release") {
        OutboxReleaseRow(group: PreviewData.uploadGroup, onCancel: {})
            .frame(width: 700)
            .padding(.vertical)
    }

    #Preview("Completed release") {
        OutboxReleaseRow(group: PreviewData.uploadGroupDone, onCancel: {})
            .frame(width: 700)
            .padding(.vertical)
    }

    #Preview("Source unavailable") {
        OutboxReleaseRow(
            group: PreviewData.uploadGroupSourceUnavailable,
            onCancel: {}
        )
        .frame(width: 700)
        .padding(.vertical)
    }
#endif
