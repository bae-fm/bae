import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Import rows whose looks the tests leave to the gallery: a row whose
    /// import waits its turn, drawing its clock, the arrow a row read from a
    /// record draws, and the sheep on the empty Import screen across its
    /// loop.
    static let importRowScenes: [ShotScene] = [
        ShotScene(
            id: "import-row-queued",
            size: CGSize(width: 340, height: 80)
        ) {
            let images = ImageStore.stub()
            return ShotStage(
                row(queuedRow)
                    .candidateReaderPreviewEnvironment()
                    .environment(images)
                    .background(Theme.background),
                images: images
            )
        },
        ShotScene(
            id: "import-rows-record-arrow",
            size: CGSize(width: 340, height: 160)
        ) {
            let images = ImageStore.stub()
            return ShotStage(
                VStack(spacing: 0) {
                    row(PreviewData.triageRowReadFromRecord)
                    row(PreviewData.triageRowNotReadFromRecord)
                }
                .candidateReaderPreviewEnvironment()
                .environment(images)
                .background(Theme.background),
                images: images
            )
        },
        importSheepMoments,
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
