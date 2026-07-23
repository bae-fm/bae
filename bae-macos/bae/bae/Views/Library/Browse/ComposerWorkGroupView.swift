import BaeKit
import SwiftUI

/// One work group under a composer: an optional parent work row, then the
/// group's works indented beneath it. Each row opens its work in the detail
/// pane.
struct ComposerWorkGroupView: View {
    let group: BridgeComposerWorkGroup
    let openWork: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(parentRows, id: \.workId) { parent in
                workRow(parent)
            }
            VStack(alignment: .leading, spacing: 2) {
                ForEach(group.works, id: \.workId) { work in
                    workRow(work)
                }
            }
            .padding(.leading, group.parent == nil ? 0 : 18)
        }
    }

    private func workRow(_ work: BridgeWorkSummary) -> some View {
        Button(action: { openWork(work.workId) }) {
            DetailMediaRow(
                image: work.representativeCover,
                title: work.title,
                subtitle: work.composerNames
            )
        }
        .buttonStyle(DetailRowButtonStyle())
    }

    private var parentRows: [BridgeWorkSummary] {
        guard let parent = group.parent else {
            return []
        }
        return [parent]
    }
}
