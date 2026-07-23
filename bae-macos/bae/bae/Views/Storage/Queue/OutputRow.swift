import BaeKit
import SwiftUI

/// One output-queue row: album title, file count, size (plus the preset name for
/// a save), a state badge (Queued / Exporting or Saving at a percent / Failed
/// with the reason in a tooltip), and a cancel button.
struct OutputRow: View {
    let op: BridgeOutputOp
    let onCancel: () -> Void

    private var presetName: String? {
        if case .save(let name) = op.kind { return name }
        return nil
    }

    var body: some View {
        QueueRow(
            icon: "square.and.arrow.up",
            createdAt: op.createdAt,
            cancelHelp: "Cancel this export",
            onCancel: onCancel
        ) {
            Text(op.title)
                .lineLimit(1)

            detailLine
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        } badge: {
            stateBadge
        }
    }

    /// "12 files · 213 MB", with the preset name appended for a save.
    @ViewBuilder
    private var detailLine: some View {
        if let presetName {
            Text("\(op.fileCount) files · \(op.totalSizeText) · \(presetName)")
        }
        else {
            Text("\(op.fileCount) files · \(op.totalSizeText)")
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        switch op.state {
        case .queued:
            Label("Queued", systemImage: "clock")
                .foregroundStyle(.secondary)
        case .active(let percent):
            activeBadge(percent: Int(percent))
                .foregroundStyle(.orange)
        case .failed(let error):
            Label("Failed", systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
                .help(error)
        }
    }

    @ViewBuilder
    private func activeBadge(percent: Int) -> some View {
        switch op.kind {
        case .export:
            Label(
                "Exporting \(percent)%",
                systemImage: "square.and.arrow.up.fill"
            )
        case .save:
            Label(
                "Saving \(percent)%",
                systemImage: "square.and.arrow.down.fill"
            )
        }
    }
}
