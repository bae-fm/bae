import BaeKit
import SwiftUI

/// One expandable queue-pane row per release (matching the storage table):
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
            HStack(spacing: 12) {
                Button {
                    expanded.toggle()
                } label: {
                    Image(
                        systemName: expanded ? "chevron.down" : "chevron.right"
                    )
                    .foregroundStyle(.secondary)
                    .frame(width: 16)
                }
                .buttonStyle(.plain)
                .help(expanded ? "Hide files" : "Show files")

                Text(group.displayTitle)
                    .lineLimit(1)

                Text(releaseDetail)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)

                Spacer()

                ProgressTrackBar(progress: group.progress.bar?.fraction ?? 0)
                    .frame(width: 140)
                    .opacity(group.progress.bar == nil ? 0 : 1)

                UploadActivityLabel(progress: group.progress)
                    .font(.caption)
                    .frame(width: 130, alignment: .leading)
            }
            .padding(.horizontal)
            .padding(.vertical, 6)
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
        guard let bar = group.progress.bar else { return files }
        return "\(files) \u{b7} \(bar.text)"
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
#endif
