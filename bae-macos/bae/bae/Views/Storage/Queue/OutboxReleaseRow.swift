import BaeKit
import SwiftUI

/// One expandable queue-pane row per release (matching the storage table):
/// the release title, its file count and cumulative byte progress, a
/// determinate progress bar, and an aggregate state badge. Expanded (the
/// default), it lists every file with its own state and live progress.
/// Right-click to cancel the release's transition. The orphaned-files bucket
/// (no release id) renders without a cancel — there's no release to act on.
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

                Text("\(group.files.count) files · \(group.progress.bytesText)")
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)

                Spacer()

                ProgressView(value: group.progress.fraction)
                    .progressViewStyle(.linear)
                    .frame(width: 140)

                stateBadge
                    .font(.caption)
                    .frame(width: 130, alignment: .leading)
            }
            .padding(.horizontal)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
            .contextMenu {
                if group.releaseId != nil {
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

    @ViewBuilder
    private var stateBadge: some View {
        let remaining = Int(group.progress.pending)
        switch group.progress.activity {
        case .queued:
            Label("Queued (\(remaining))", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .uploading:
            Label(
                "Uploading (\(remaining))",
                systemImage: "arrow.up.circle.fill"
            )
            .foregroundStyle(.orange)
        case .retrying:
            Label(
                "Retrying (\(remaining))",
                systemImage: "exclamationmark.triangle.fill"
            )
            .foregroundStyle(.red)
        case .none:
            EmptyView()
        }
    }
}
