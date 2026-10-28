import BaeKit
import SwiftUI

/// A release transfer's counts on one line, as core lists them, joined by
/// dots: the files on the move in the transfer's colour, those waiting in
/// the line's colour, those done dimmed.
struct StorageTransferCounts: View {
    let counts: [BridgeTransferCount]
    let kind: StorageTransferKind

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            ForEach(Array(counts.enumerated()), id: \.element.label.key) {
                index,
                count in
                if index > 0 {
                    Text(verbatim: "\u{00B7}").foregroundStyle(.secondary)
                }
                Text(
                    QueueSummary.countLabel(count.label.key, count.label.count)
                )
                .fontWeight(count.stage == .moving ? .semibold : .regular)
                .foregroundStyle(style(count.stage))
            }
        }
    }

    private func style(_ stage: BridgeTransferStage) -> AnyShapeStyle {
        switch stage {
        case .moving: AnyShapeStyle(kind.color)
        case .waiting: AnyShapeStyle(.primary)
        case .done: AnyShapeStyle(.secondary)
        }
    }
}
