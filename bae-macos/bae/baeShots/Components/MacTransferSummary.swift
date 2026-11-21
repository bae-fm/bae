import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTransferSummary`, an upload with one file moving, two waiting
    /// and four done.
    static let macTransferSummaryScenes: [ShotScene] = [
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
}
