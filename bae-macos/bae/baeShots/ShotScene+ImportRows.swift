import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Import rows whose looks the tests leave to the gallery: a row whose
    /// import waits its turn, drawing its clock, and the arrow a row read
    /// from a record draws, at rest and selected.
    static let importRowScenes: [ShotScene] = [
        ShotScene(
            id: "import-row-queued",
            size: CGSize(width: 340, height: 80)
        ) {
            AnyView(
                row(queuedRow)
                    .candidateReaderPreviewEnvironment()
                    .environment(ImageStore.stub())
                    .background(Theme.background)
            )
        },
        ShotScene(
            id: "import-rows-record-arrow",
            size: CGSize(width: 340, height: 240)
        ) {
            AnyView(
                VStack(spacing: 0) {
                    row(PreviewData.triageRowReadFromRecord)
                    row(PreviewData.triageRowNotReadFromRecord)
                    row(PreviewData.triageRowReadFromRecord)
                        .environment(\.backgroundProminence, .increased)
                        .background(Color.accentColor)
                }
                .candidateReaderPreviewEnvironment()
                .environment(ImageStore.stub())
                .background(Theme.background)
            )
        },
    ]

    private static func row(_ row: BridgeTriageRow) -> some View {
        TriageRowView(row: row, coverContent: nil, isGroupMember: false)
            .frame(height: 80)
    }

    /// A row filled from tags whose import waits its turn.
    private static var queuedRow: BridgeTriageRow {
        var row = PreviewData.triageRowPrefilledFromTags
        row.live = BridgeCandidateLiveState(
            identifying: nil,
            readingPick: false,
            import: .preparing(step: .queued),
            actions: [],
            standing: .notLookedUp,
            badge: nil
        )
        return row
    }
}
