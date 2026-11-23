import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageBand`, a release's storage in its Storage sheet, in each
    /// state the band draws. The `why` row's reason is the one core gives for
    /// a release whose audio is on another device: core has no reason about
    /// FLAC files over 24 bits.
    static let macStorageBandScenes: [ShotScene] = [
        storageBand("mac.component.storage-band.in-cloud", height: 120),
        storageBand(
            "mac.component.storage-band.local",
            state: .onOneDevice,
            height: 120
        ),
        storageBand(
            "mac.component.storage-band.pinned",
            state: .inStorage(pinned: true),
            height: 120
        ),
        storageBand(
            "mac.component.storage-band.uploading",
            state: .onOneDevice,
            upload: shotUpload(queued: 0, uploading: 1),
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.files-waiting",
            state: .onOneDevice,
            upload: shotUpload(queued: 3, uploading: 0),
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.original-missing",
            state: .onOneDevice,
            upload: shotUpload(queued: 3, uploading: 0, missing: "disc 2.flac"),
            height: 150
        ),
        storageBand(
            "mac.component.storage-band.pin-queued",
            download: .queued,
            height: 120
        ),
        storageBand(
            "mac.component.storage-band.pinning",
            download: .downloading,
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.pin-failed",
            download: .failed(error: moveFailure),
            height: 190
        ),
        storageBand(
            "mac.component.storage-band.queued-to-move-to-this-mac",
            move: .queued,
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.moving-to-this-mac",
            move: .making,
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.could-not-move-to-this-mac",
            move: .failed(error: moveFailure),
            height: 190
        ),
        storageBand(
            "mac.component.storage-band.why",
            playability: .onOtherDevice(device: .active(name: "Studio Mac")),
            height: 150
        ),
    ]

    private static func storageBand(
        _ id: String,
        state: BridgeReleaseStorageState = .inStorage(pinned: false),
        upload: BridgeReleaseUpload? = nil,
        download: BridgeReleaseDownloadStatus? = nil,
        move: BridgeMoveToThisDeviceStatus? = nil,
        playability: BridgeReleasePlayability = .playable,
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: 596,
            height: height
        ) {
            var bridge = PreviewData.storageBridgeRelease(
                storageState: state,
                storageActions: [.pin, .moveToThisDevice],
                playability: playability
            )
            bridge.upload = upload
            bridge.downloadStatus = download
            bridge.moveToThisDevice = move
            // A queued pin and a failure still offer their actions; an
            // upload, a running pin and a move to This Mac, queued or running,
            // offer none, as core's `transitioning` says.
            bridge.transitioning =
                upload != nil || download == .downloading || move == .queued
                || move == .making
            let bytes: UInt64 = 344_000_000
            return MacStorageBand(
                release: ReleaseDetail(bridge: bridge),
                onAction: { _ in },
                onCancelPin: {},
                onExport: {},
                onSaveAs: {}
            )
            .storageBandTransfers(of: bridge.id, bytes: bytes)
        }
    }
}

extension View {
    /// The transfers a Storage band's bars read: an upload, a pin and a move
    /// to This Mac, each 40 percent through `bytes` of the release's.
    fileprivate func storageBandTransfers(
        of releaseId: String,
        bytes: UInt64
    ) -> some View {
        self
            .environment(
                OutboxStore(
                    uploadsPaused: false,
                    releaseProgress: .releaseUploads(
                        holding: [
                            releaseId: BridgeReleaseUploadTransfer(
                                bar: BridgeUploadBar(
                                    bytesDone: bytes * 2 / 5,
                                    bytesTotal: bytes,
                                    fraction: 0.4
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
                            releaseId: BridgeDownloadTransferProgress(
                                bytesDone: bytes * 2 / 5,
                                bytesTotal: bytes,
                                fraction: 0.4,
                                throughputBps: 3_100_000
                            )
                        ]
                    ),
                    moveToThisDeviceTransfers: .movesToThisDevice(
                        holding: [
                            releaseId: BridgeMoveToThisDeviceProgress(
                                file: BridgeMoveToThisDeviceFileProgress(
                                    bytesDone: 10_000_000,
                                    bytesTotal: 34_000_000
                                ),
                                bytesDone: bytes * 2 / 5,
                                bytesTotal: bytes,
                                fraction: 0.4
                            )
                        ]
                    )
                )
            )
    }
}
