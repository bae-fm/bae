import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of Settings → Storage and the storage inspector
    /// that draw from values: the usage part, a release's cell, a transfer's
    /// summary, and the cast picker's popover.
    static let storageComponentScenes: [ShotScene] =
        storageUsageScenes + releaseCellScenes + transferSummaryScenes
        + castPopoverScenes + listRowScenes + scanCodeScenes
        + storageBandScenes

    private static func storageUsage(
        _ id: String,
        _ storage: BridgeDeviceStorage,
        height: CGFloat
    ) -> ShotScene {
        component(id, width: 596, height: height) {
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
                                    bytesTotal: 221_200_000
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

    private static func castReceiver(
        _ name: String,
        _ kind: BridgeRendererKind
    ) -> BridgeCastReceiver {
        BridgeCastReceiver(
            id: name,
            name: name,
            kind: kind,
            model: nil,
            address: "192.168.1.41",
            takesVolume: true
        )
    }

    /// `CastPickerPopover`, the cast button's popover, over a receiver
    /// cast to with a short name and with a long one.
    private static let castPopoverScenes: [ShotScene] = [
        component("mac.component.cast-popover.default", height: 560) {
            VStack(alignment: .leading, spacing: 20) {
                CastPickerPopover(
                    receivers: BridgeCastReceiverList(
                        casting: castReceiver("Living Room", .cast),
                        found: [
                            castReceiver("Kitchen", .airPlay),
                            castReceiver("Den receiver", .dlna),
                        ]
                    ),
                    endedFailure: nil,
                    connectingName: nil,
                    onDisconnect: {}
                )
                CastPickerPopover(
                    receivers: BridgeCastReceiverList(
                        casting: castReceiver(
                            "Upstairs Living Room Sonos Arc Soundbar",
                            .cast
                        ),
                        found: [
                            castReceiver("Kitchen", .airPlay),
                            castReceiver(
                                "Denon AVR-X3800H Home Theater Receiver (Basement)",
                                .dlna
                            ),
                        ]
                    ),
                    endedFailure: nil,
                    connectingName: nil,
                    onDisconnect: {}
                )
            }
            .environment(CastStart.stub())
        }
    ]

    /// `MacListRow`, a track on a playlist's page.
    private static let listRowScenes: [ShotScene] = [
        component(
            "mac.component.list-row.default",
            height: 70,
            images: coverStore()
        ) {
            MacListRow(
                lead: "21:04",
                cover: cover("Low Tide Hours"),
                title: "Harbor Lights",
                subtitle: "Mara Quill · Low Tide Hours",
                duration: "4:32"
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    /// `MacScanCode`, the QR code of a short code with the buttons that
    /// hand it over.
    private static let scanCodeScenes: [ShotScene] = [
        component("mac.component.scan-code.default", height: 330) {
            MacScanCode(
                instruction:
                    "Scan the QR code on their device, or share the code.",
                qr: "bae-pairing-4F7K-2M9Q",
                code: "4F7K-2M9Q"
            )
            .frame(width: 380, alignment: .leading)
        }
    ]

    private static func storageBand(
        _ id: String,
        state: BridgeReleaseStorageState = .inStorage(pinned: false),
        actions: [BridgeReleaseStorageAction] = [],
        upload: BridgeReleaseUpload? = nil,
        download: BridgeReleaseDownloadStatus? = nil,
        move: BridgeMoveToThisDeviceStatus? = nil,
        height: CGFloat
    ) -> ShotScene {
        component(id, width: 596, height: height) {
            var bridge = PreviewData.storageBridgeRelease(
                storageState: state,
                storageActions: actions,
                playability: .playable
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
            return StorageStatusBand(
                release: ReleaseDetail(bridge: bridge),
                onAction: { _ in },
                onCancelPin: {},
                onExport: {},
                onSaveAs: {}
            )
            .environment(
                OutboxStore(
                    uploadsPaused: false,
                    releaseProgress: .releaseUploads(
                        holding: [
                            bridge.id: BridgeReleaseUploadTransfer(
                                bar: BridgeUploadBar(
                                    bytesDone: 96_000_000,
                                    bytesTotal: 221_200_000
                                ),
                                throughputBps: 6_400_000
                            )
                        ]
                    )
                )
            )
            .environment(DownloadStore())
        }
    }

    private static func upload(
        queued: UInt32,
        uploading: UInt32
    ) -> BridgeReleaseUpload {
        BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: queued,
                uploading: uploading,
                activity: uploading > 0 ? .uploading : .queued
            ),
            summary: [],
            missing: nil
        )
    }

    /// `StorageStatusBand`, a release's storage in its Storage sheet, in
    /// each state the band draws.
    private static let storageBandScenes: [ShotScene] = [
        storageBand(
            "mac.component.storage-band.resting",
            actions: [.pin, .moveToThisDevice],
            height: 120
        ),
        storageBand(
            "mac.component.storage-band.uploading",
            state: .onOneDevice,
            upload: upload(queued: 0, uploading: 1),
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.files-waiting",
            state: .onOneDevice,
            upload: upload(queued: 3, uploading: 0),
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.pin-queued",
            actions: [.pin, .moveToThisDevice],
            download: .queued,
            height: 120
        ),
        storageBand(
            "mac.component.storage-band.pinning",
            download: .downloading,
            height: 90
        ),
        storageBand(
            "mac.component.storage-band.download-failed",
            actions: [.pin, .moveToThisDevice],
            download: .failed(error: "The network connection was lost."),
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
            actions: [.pin, .moveToThisDevice],
            move: .failed(error: "The network connection was lost."),
            height: 190
        ),
    ]
}
