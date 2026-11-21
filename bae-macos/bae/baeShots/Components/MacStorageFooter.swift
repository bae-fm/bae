import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageFooter`: the count and total alone, with the three queues
    /// moving, and with two held back. An export has no rate in the app: its
    /// queue counts the percent written, where the component says
    /// "18 MB/s".
    static let macStorageFooterScenes: [ShotScene] = [
        storageFooter(
            "mac.component.storage-footer.releases-only",
            queues: BridgeStorageQueues(
                uploads: nil,
                downloads: nil,
                outputs: nil
            )
        ),
        storageFooter(
            "mac.component.storage-footer.queues-moving",
            queues: BridgeStorageQueues(
                uploads: BridgeUploadQueueRun(wait: nil),
                downloads: BridgeReleaseQueueRun(
                    wait: nil,
                    active: "rel-row-1"
                ),
                outputs: BridgeReleaseQueueRun(wait: nil, active: "rel-row-2")
            )
        ),
        storageFooter(
            "mac.component.storage-footer.queues-held",
            queues: BridgeStorageQueues(
                uploads: BridgeUploadQueueRun(wait: .offline),
                downloads: BridgeReleaseQueueRun(wait: .paused, active: nil),
                outputs: nil
            )
        ),
    ]

    /// `MacStorageFooter` over the board's twelve releases, 4.4 GB, with the
    /// transfer queues `queues` names and the streams their rates read:
    /// uploads at 6.4 MB/s, pins at 3.1 MB/s and exports 18 percent written.
    private static func storageFooter(
        _ id: String,
        queues: BridgeStorageQueues
    ) -> ShotScene {
        ShotScene(
            id: id,
            size: CGSize(width: 640, height: 40)
        ) {
            var rows = boardStorageRows
            rows[11].release.totalSize = 250_000_000
            let fixture = StorageManagerPreviewFixture(
                rows: rows,
                outputs: [],
                queues: queues
            )
            let footer = ShotStorageList(store: fixture.storageManagerStore) {
                store,
                list in
                MacStorageFooter(
                    list: list,
                    totalSize: store.totalSize,
                    queues: store.queues
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Theme.surfaceElevated)
            return ShotStage(
                fixture.environment(
                    of: footer,
                    downloads: DownloadStore(
                        releaseProgress: .downloads(
                            holding: [
                                "rel-row-1": BridgeDownloadTransferProgress(
                                    bytesDone: 140_000_000,
                                    bytesTotal: 367_000_000,
                                    fraction: 140.0 / 367.0,
                                    throughputBps: 3_100_000
                                )
                            ]
                        )
                    ),
                    outputs: OutputStore(
                        percents: .exportPercents(holding: ["rel-row-2": 18])
                    ),
                    outbox: OutboxStore(
                        uploadsPaused: false,
                        filesBeingSent: .uploadTotal(
                            holding: BridgeUploadTransfer(
                                bar: BridgeUploadBar(
                                    bytesDone: 22_400_000,
                                    bytesTotal: 57_100_000,
                                    fraction: 22.4 / 57.1
                                ),
                                throughputBps: 6_400_000
                            )
                        )
                    )
                )
            ) {
                fixture.storageManagerStore.list?.count != nil
                    && fixture.storageManagerStore.queues != nil
            }
        }
    }
}
