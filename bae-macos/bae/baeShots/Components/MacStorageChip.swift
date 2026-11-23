import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageChip` in each state a release's Storage column shows. The
    /// rows that add how many wait ahead or what holds the queue (`ahead`,
    /// `wait`) show what the rows without them show: the component draws
    /// neither, and neither does the app.
    static let macStorageChipScenes: [ShotScene] = [
        storageChip("mac.component.storage-chip.at-rest") {
            statusChip(.onOneDevice)
            statusChip(.pinned)
            statusChip(.inStorage)
        },
        storageChip("mac.component.storage-chip.moving") {
            uploadingChip()
            pinningChip()
        },
        storageChip("mac.component.storage-chip.download-failed") {
            statusChip(.downloadFailed(error: moveFailure))
        },
        storageChip("mac.component.storage-chip.move-failed") {
            statusChip(.moveFailed(fileName: "01 Harbor Lights.flac"))
        },
        storageChip("mac.component.storage-chip.could-not-move-to-this-mac") {
            statusChip(.moveToThisDeviceFailed(error: moveFailure))
        },
        storageChip("mac.component.storage-chip.queued") { queuedUploadChip() },
        storageChip("mac.component.storage-chip.long") {
            queuedUploadChip(long: true)
            statusChip(.pinning(step: .queued), long: true)
            statusChip(.movingToThisDevice(step: .queued), long: true)
            movingChip(long: true)
        },
        storageChip("mac.component.storage-chip.queued-to-move-to-this-mac") {
            statusChip(.movingToThisDevice(step: .queued))
        },
        storageChip("mac.component.storage-chip.upload-blocked") {
            queuedUploadChip()
        },
    ]

    private static let chipUploadingId = "rel-chip-upload"

    private static let chipPinningId = "rel-chip-pin"

    private static let chipMovingId = "rel-chip-move"

    /// A chip scene: `chips` in a row, over the transfers each moving chip
    /// reads: an upload 60 percent sent, a pin 30 percent downloaded and a
    /// move to This Mac 45 percent done.
    private static func storageChip(
        _ id: String,
        @ViewBuilder chips: @escaping @MainActor () -> some View
    ) -> ShotScene {
        component(id, height: 50) {
            HStack(spacing: 6) { chips() }
                .environment(
                    OutboxStore(
                        uploadsPaused: false,
                        releaseProgress: .releaseUploads(
                            holding: [
                                chipUploadingId: BridgeReleaseUploadTransfer(
                                    bar: BridgeUploadBar(
                                        bytesDone: 60_000_000,
                                        bytesTotal: 100_000_000,
                                        fraction: 0.6
                                    ),
                                    throughputBps: 6_400_000
                                )
                            ]
                        )
                    )
                )
                .environment(
                    DownloadStore(
                        releaseProgress: .downloads(
                            holding: [
                                chipPinningId: BridgeDownloadTransferProgress(
                                    bytesDone: 30_000_000,
                                    bytesTotal: 100_000_000,
                                    fraction: 0.3,
                                    throughputBps: 3_100_000
                                )
                            ]
                        ),
                        moveToThisDeviceTransfers: .movesToThisDevice(
                            holding: [
                                chipMovingId: BridgeMoveToThisDeviceProgress(
                                    file: BridgeMoveToThisDeviceFileProgress(
                                        bytesDone: 10_000_000,
                                        bytesTotal: 34_000_000
                                    ),
                                    bytesDone: 45_000_000,
                                    bytesTotal: 100_000_000,
                                    fraction: 0.45
                                )
                            ]
                        )
                    )
                )
        }
    }

    /// The chip of a release at rest or failed, by its status.
    private static func statusChip(
        _ status: BridgeStorageRowStatus,
        long: Bool = false
    ) -> MacStorageChip {
        MacStorageChip(
            release: PreviewData.storageRelease(),
            status: status,
            long: long
        )
    }

    /// The chip of a release whose upload waits its turn.
    private static func queuedUploadChip(long: Bool = false) -> MacStorageChip {
        MacStorageChip(
            release: PreviewData.storageRelease(
                upload: shotUpload(queued: 3, uploading: 0)
            ),
            status: .uploading,
            long: long
        )
    }

    private static func uploadingChip() -> MacStorageChip {
        MacStorageChip(
            release: PreviewData.storageRelease(
                id: chipUploadingId,
                upload: shotUpload(queued: 0, uploading: 1)
            ),
            status: .uploading
        )
    }

    private static func pinningChip() -> MacStorageChip {
        MacStorageChip(
            release: PreviewData.storageRelease(
                id: chipPinningId,
                transitioning: true
            ),
            status: .pinning(step: .running)
        )
    }

    private static func movingChip(long: Bool = false) -> MacStorageChip {
        MacStorageChip(
            release: PreviewData.storageRelease(
                id: chipMovingId,
                transitioning: true
            ),
            status: .movingToThisDevice(step: .running),
            long: long
        )
    }
}
