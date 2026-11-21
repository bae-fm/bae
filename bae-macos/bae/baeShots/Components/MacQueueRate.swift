import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacQueueRate`, a queue's rate and a queue held back. An export has
    /// no rate in the app: its queue counts the percent written, where the
    /// component says "18 MB/s".
    static let macQueueRateScenes: [ShotScene] = [
        queueRate("mac.component.queue-rate.uploads", .uploads),
        queueRate("mac.component.queue-rate.pins", .downloads),
        queueRate("mac.component.queue-rate.exports", .outputs),
        queueRate(
            "mac.component.queue-rate.waiting",
            .uploads,
            hold: BridgeUploadQueueWait.offline.text
        ),
        queueRate(
            "mac.component.queue-rate.retrying",
            .uploads,
            hold: BridgeUploadQueueWait.retrying(atMs: retryTimeMs).text
        ),
        queueRate(
            "mac.component.queue-rate.paused",
            .downloads,
            hold: BridgeQueueWait.paused.text,
            paused: true
        ),
    ]

    /// One transfer queue's rate or hold, over the streams the app's footer
    /// reads: uploads 6.4 MB/s, pins 3.1 MB/s, exports 18 percent.
    private static func queueRate(
        _ id: String,
        _ queue: StorageQueueKind,
        hold: String? = nil,
        paused: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            let outbox = OutboxStore(
                uploadsPaused: false,
                filesBeingSent: .uploadTotal(
                    holding: BridgeUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 140_000_000,
                            bytesTotal: 356_000_000,
                            fraction: 0.4
                        ),
                        throughputBps: 6_400_000
                    )
                )
            )
            let downloads = DownloadStore(
                releaseProgress: .downloads(
                    holding: [
                        "rel-rate": BridgeDownloadTransferProgress(
                            bytesDone: 12_000_000,
                            bytesTotal: 344_000_000,
                            fraction: 0.1,
                            throughputBps: 3_100_000
                        )
                    ]
                )
            )
            let outputs = OutputStore(
                percents: .exportPercents(holding: ["rel-rate": 18])
            )
            MacQueueRate(queue: queue, paused: paused, hold: hold) {
                switch queue {
                case .uploads:
                    StreamLabel(stream: .uploadRate(outbox.totalProgress))
                case .downloads:
                    StreamLabel(
                        stream: .pinRate(
                            downloads.downloadProgress(forRelease: "rel-rate")
                        )
                    )
                case .outputs:
                    StreamLabel(
                        stream: .exportPercent(
                            outputs.exportProgress(forRelease: "rel-rate")
                        )
                    )
                }
            }
            .environment(outbox)
            .environment(downloads)
            .environment(outputs)
            .environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(UiStore())
        }
    }

    /// 6:40 PM today, as the upload queue's retry time.
    private static let retryTimeMs: Int64 = {
        let time =
            Calendar.current.date(
                bySettingHour: 18,
                minute: 40,
                second: 0,
                of: Date()
            ) ?? Date()
        return Int64(time.timeIntervalSince1970 * 1000)
    }()
}
