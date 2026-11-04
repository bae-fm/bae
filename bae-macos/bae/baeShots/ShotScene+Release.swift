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
            AnyView(
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
            AnyView(
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
            return AnyView(
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
                        releaseDetail: { .fixed { _ in release } },
                        releaseFiles: {
                            .fixed(PreviewData.previewReleaseFiles)
                        }
                    )
                )
                .environment(OutboxStore(uploadsPaused: false))
                .environment(DownloadStore())
            )
        },
    ]
}
