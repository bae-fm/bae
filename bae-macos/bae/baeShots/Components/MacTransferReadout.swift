import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTransferReadout`, its bytes and rate and its bar read from the
    /// transfer's stream by native views, as the app draws them.
    static let macTransferReadoutScenes: [ShotScene] = [
        component("mac.component.transfer-readout.upload", height: 60) {
            let outbox = OutboxStore(
                uploadsPaused: false,
                releaseProgress: .releaseUploads(
                    holding: [
                        "rel-readout": BridgeReleaseUploadTransfer(
                            bar: BridgeUploadBar(
                                bytesDone: 140_000_000,
                                bytesTotal: 356_000_000,
                                fraction: 0.4
                            ),
                            throughputBps: 6_400_000
                        )
                    ]
                )
            )
            MacTransferReadout(
                stream: .upload(
                    outbox.uploadProgress(forRelease: "rel-readout")
                ),
                kind: .upload
            )
            .frame(width: 380)
            .environment(outbox)
        },
        component("mac.component.transfer-readout.pin", height: 60) {
            let downloads = DownloadStore(
                releaseProgress: .downloads(
                    holding: [
                        "rel-readout": BridgeDownloadTransferProgress(
                            bytesDone: 12_000_000,
                            bytesTotal: 344_000_000,
                            fraction: 0.1,
                            throughputBps: 3_100_000
                        )
                    ]
                )
            )
            MacTransferReadout(
                stream: .pin(
                    downloads.downloadProgress(forRelease: "rel-readout")
                ),
                kind: .pin
            )
            .frame(width: 380)
            .environment(downloads)
        },
    ]
}
