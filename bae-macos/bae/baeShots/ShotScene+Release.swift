import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// A release's records and storage: the records row naming every
    /// catalog, the card behind the facts line, and the Storage sheet over a
    /// pinned release.
    static let releaseScenes: [ShotScene] = [
        ShotScene(
            id: "release-records-every-catalog",
            size: CGSize(width: 420, height: 120)
        ) {
            ShotStage(
                ReleaseRecordsRow(
                    records: PreviewData.releaseRecordsEveryCatalog
                )
                .padding()
            )
        },
        ShotScene(
            id: "release-records-card",
            size: CGSize(width: ReleaseRecordsCard.width, height: 96)
        ) {
            ShotStage(
                ReleaseRecordsCard(
                    records: PreviewData.identifiedFromBothCatalogs
                )
            )
        },
        ShotScene(
            id: "release-storage-sheet",
            size: CGSize(width: 640, height: 600)
        ) {
            let release = PreviewData.storageBridgeRelease(
                storageState: .inStorage(pinned: true),
                storageActions: [.unpin, .detachAudio],
                playability: .playable
            )
            let answers = FixedAnswers()
            // The sheet asks for the release and its files as it appears,
            // a main-actor step after it is first drawn, which the tree's
            // coming to rest spans.
            return ShotStage(
                ReleaseStorageSheet(
                    release: ReleaseDetail(bridge: release),
                    onAction: { _ in },
                    onExport: {},
                    onSaveAs: {},
                    onDone: {}
                )
                .background(Theme.background)
                .environment(
                    Library(
                        releaseDetail: {
                            .fixed(answers: answers) { _ in release }
                        },
                        releaseFiles: {
                            .fixed(
                                PreviewData.previewReleaseFiles,
                                answers: answers
                            )
                        }
                    )
                )
                .environment(OutboxStore(uploadsPaused: false))
                .environment(DownloadStore())
            ) { answers.unanswered == 0 }
        },
    ]
}
