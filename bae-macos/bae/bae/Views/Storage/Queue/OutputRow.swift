import BaeKit
import SwiftUI

/// One output-queue row: album title, file count, size, the preset name for a
/// save, a state badge, and a cancel button.
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
            cancel: .init(help: "Cancel this export", action: onCancel)
        ) {
            Text(op.titleText)
                .lineLimit(1)

            detailLine
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        } badge: {
            stateBadge
        }
    }

    /// "12 files · 213 MB", with the preset name appended for a save; only
    /// the preset name once the library no longer holds the release.
    @ViewBuilder
    private var detailLine: some View {
        switch (op.release, presetName) {
        case (let release?, let presetName?):
            Text(
                "\(release.fileCount) files · \(release.totalSizeText) · \(presetName)"
            )
        case (let release?, nil):
            Text("\(release.fileCount) files · \(release.totalSizeText)")
        case (nil, let presetName?):
            Text(presetName)
        case (nil, nil):
            EmptyView()
        }
    }

    @ViewBuilder
    private var stateBadge: some View {
        switch op.state {
        case .queued:
            StatusChip("Queued", symbol: "clock")
        case .active(let percent):
            activeBadge(percent: Int(percent))
        case .failed(let error):
            StatusChip(
                "Failed",
                tone: .danger,
                symbol: StatusTone.danger.symbol
            )
            .help(error)
        }
    }

    @ViewBuilder
    private func activeBadge(percent: Int) -> some View {
        switch op.kind {
        case .export:
            StatusChip(
                "Exporting \(percent)%",
                tone: .activity,
                symbol: "square.and.arrow.up.fill"
            )
        case .save:
            StatusChip(
                "Saving \(percent)%",
                tone: .activity,
                symbol: "square.and.arrow.down.fill"
            )
        }
    }
}

#if DEBUG
    #Preview("Output states") {
        VStack(spacing: 0) {
            ForEach(PreviewData.outputOps, id: \.releaseId) { op in
                OutputRow(op: op, onCancel: {})
                Divider()
            }
        }
        .frame(width: 700)
        .padding(.vertical)
    }
#endif
