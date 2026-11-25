import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacFailedTransferRow`: a pin and a move to This Mac that failed, an
    /// upload held by a missing original, and a move to the cloud that
    /// failed because an original changed.
    static let macFailedTransferRowScenes: [ShotScene] = [
        failedTransferRow(
            "mac.component.failed-transfer-row.pin",
            entry: .download(failedDownload(.pin), onRetry: {}),
            height: 130
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.move-to-this-mac",
            entry: .download(failedDownload(.moveToThisDevice), onRetry: {}),
            height: 130
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.upload-waiting",
            entry: .uploadWaiting(
                title: "Glass Harbor",
                fileCount: 9,
                totalSize: 356_000_000,
                missing: "01 Harbor Lights.flac"
            ),
            height: 150
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.move-to-the-cloud-failed",
            entry: .moveFailed(
                title: "Glass Harbor",
                fileCount: 9,
                totalSize: 356_000_000,
                changed: "01 Harbor Lights.flac",
                reason: DisplayError(
                    line: "Could not move to the cloud",
                    detail: "The original changed after the move began."
                )
            ),
            height: 150
        ),
    ]

    private static func failedDownload(
        _ kind: BridgeDownloadKind
    ) -> BridgeDownloadOp {
        BridgeDownloadOp(
            releaseId: "rel-glass-harbor",
            kind: kind,
            fileCount: 9,
            totalSize: 356_000_000,
            release: BridgeQueuedRelease(title: "Glass Harbor"),
            createdAt: 0,
            state: .failed(error: "The network connection was lost.")
        )
    }

    private static func failedTransferRow(
        _ id: String,
        entry: MacFailedTransferRow.Entry,
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: 440,
            height: height
        ) {
            MacFailedTransferRow(entry: entry, onCancel: {})
        }
    }
}
