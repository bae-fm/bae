import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of Settings → Storage, the storage inspector and
    /// the Storage Manager that draw from values: the usage part and its
    /// keys, a release's cell, a transfer's summary, a release's storage
    /// band and the storage chip.
    static let storageComponentScenes: [ShotScene] =
        storageUsageScenes + usageKeyScenes + releaseCellScenes
        + transferSummaryScenes + scanCodeScenes + storageStateScenes
        + storageBandScenes + storageChipScenes + storageFileComponentScenes

    private static func storageUsage(
        _ id: String,
        _ storage: BridgeDeviceStorage,
        height: CGFloat
    ) -> ShotScene {
        component(id, width: 580, height: height) {
            MacStorageUsage(usage: StorageUsage(storage))
        }
    }

    /// `MacStorageUsage`, with and without data a reset left, and with
    /// nothing pinned.
    private static let storageUsageScenes: [ShotScene] = [
        storageUsage(
            "mac.component.storage-usage.usual",
            .previewUsual,
            height: 130
        ),
        storageUsage(
            "mac.component.storage-usage.from-before-a-reset",
            .previewBeforeReset,
            height: 150
        ),
        storageUsage(
            "mac.component.storage-usage.nothing-pinned",
            .previewNothingPinned,
            height: 110
        ),
    ]

    /// `MacReleaseCell`, a release's cover and title.
    private static let releaseCellScenes: [ShotScene] = [
        component(
            "mac.component.release-cell.default",
            page: true,
            height: 50,
            images: coverStore()
        ) {
            MacReleaseCell(
                release: PreviewData.storageRelease(
                    cover: cover("Low Tide Hours")
                ),
                album: BridgeAlbum(
                    id: "album-store-1",
                    title: "Low Tide Hours",
                    year: 1992,
                    isCompilation: false,
                    artistLine: .named("Mara Quill"),
                    releaseIds: [],
                    primaryReleaseId: "rel-store-1",
                    cover: cover("Low Tide Hours")
                )
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    /// `MacTransferSummary`, an upload with one file moving, two waiting
    /// and four done.
    private static let transferSummaryScenes: [ShotScene] = [
        component("mac.component.transfer-summary.default", height: 110) {
            MacTransferSummary(
                kind: .upload,
                counts: [
                    PreviewData.transferCount(
                        "core.queue.uploading",
                        1,
                        .moving
                    ),
                    PreviewData.transferCount(
                        "core.queue.queued_to_upload",
                        2,
                        .waiting
                    ),
                    PreviewData.transferCount(
                        "core.queue.stored",
                        4,
                        .done
                    ),
                ],
                releaseId: "rel-summary",
                moving: true,
                onCancel: {}
            )
            .frame(width: 380, alignment: .leading)
            .environment(
                OutboxStore(
                    uploadsPaused: false,
                    releaseProgress: .releaseUploads(
                        holding: [
                            "rel-summary": BridgeReleaseUploadTransfer(
                                bar: BridgeUploadBar(
                                    bytesDone: 96_000_000,
                                    bytesTotal: 221_200_000,
                                    fraction: 96 / 221.2
                                ),
                                throughputBps: 6_400_000
                            )
                        ]
                    )
                )
            )
            .environment(DownloadStore())
        }
    ]

    /// `MacScanCode`, the QR code of a short code with the buttons that copy
    /// and share it.
    private static let scanCodeScenes: [ShotScene] = [
        component("mac.component.scan-code.default", page: true, height: 330) {
            MacScanCode(
                instruction:
                    "Scan the QR code on their device, or share the code.",
                qr: "bae-pairing-4F7K-2M9Q",
                code: "4F7K-2M9Q"
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    /// `MacUsageKey`, one kind of the usage legend, in each colour core's
    /// kinds take. The component's fifth row, a kind the legend has no
    /// colour for, has no kind in the app: the app's four kinds are the
    /// rows above.
    private static let usageKeyScenes: [ShotScene] = [
        usageKey("mac.component.usage-key.pinned", .pinned, "6.1 GB"),
        usageKey("mac.component.usage-key.cache", .cache, "1.8 GB"),
        usageKey("mac.component.usage-key.library", .library, "420 MB"),
        usageKey(
            "mac.component.usage-key.from-before-the-reset",
            .beforeReset,
            "960 MB"
        ),
    ]

    private static func usageKey(
        _ state: String,
        _ kind: BridgeStorageKind,
        _ size: String
    ) -> ShotScene {
        component(state, height: 40) {
            MacUsageKey(color: kind.color, title: kind.title, size: size)
        }
    }

    /// `MacStorageState`, a release's storage at rest.
    private static let storageStateScenes: [ShotScene] = [
        storageState("mac.component.storage-state.local", .onOneDevice),
        storageState(
            "mac.component.storage-state.pinned",
            .inStorage(pinned: true)
        ),
        storageState(
            "mac.component.storage-state.in-cloud",
            .inStorage(pinned: false)
        ),
    ]

    private static func storageState(
        _ id: String,
        _ state: BridgeReleaseStorageState
    ) -> ShotScene {
        component(id, height: 40) {
            MacStorageState(state: state)
        }
    }

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

    /// A release's upload with `queued` files waiting and `uploading` moving.
    static func shotUpload(
        queued: UInt32,
        uploading: UInt32,
        missing: String? = nil
    ) -> BridgeReleaseUpload {
        BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: queued,
                uploading: uploading,
                activity: uploading > 0 ? .uploading : .queued
            ),
            summary: [],
            missing: missing
        )
    }

    private static let moveFailure = "The network connection was lost."

    /// `MacStorageBand`, a release's storage in its Storage sheet, in each
    /// state the band draws. The `why` row's reason is the one core gives for
    /// a release whose audio is on another device: core has no reason about
    /// FLAC files over 24 bits.
    private static let storageBandScenes: [ShotScene] = [
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
                                    fileBytesDone: 10_000_000,
                                    fileBytesTotal: 34_000_000,
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

    /// `MacStorageChip` in each state a release's Storage column shows. The
    /// rows that add how many wait ahead or what holds the queue (`ahead`,
    /// `wait`) show what the rows without them show: the component draws
    /// neither, and neither does the app.
    private static let storageChipScenes: [ShotScene] = [
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
    ]
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
                                fileBytesDone: 10_000_000,
                                fileBytesTotal: 34_000_000,
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
