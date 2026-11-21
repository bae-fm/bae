import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTransferLine` in the Activity panel's 332pt column.
    static let macTransferLineScenes: [ShotScene] = [
        transferLine(
            "mac.component.transfer-line.syncing",
            title: "Syncing changes",
            count: nil,
            queue: nil,
            share: nil
        ),
        transferLine(
            "mac.component.transfer-line.uploading",
            title: "Uploading",
            count: "1 of 3 releases",
            queue: .init(.uploads, paused: false),
            share: 0.4
        ),
        transferLine(
            "mac.component.transfer-line.waiting",
            title: "Uploading",
            count: "Waiting for Wi-Fi",
            queue: .init(.uploads, paused: false),
            share: 0.4
        ),
        transferLine(
            "mac.component.transfer-line.paused",
            title: "Pinning",
            count: "Paused",
            queue: .init(.downloads, paused: true),
            share: 0.3
        ),
        transferLine(
            "mac.component.transfer-line.moving-to-this-mac",
            title: "Moving to This Mac",
            count: "0 of 1 release",
            queue: .init(.downloads, paused: false),
            share: 0.2
        ),
    ]

    private static func transferLine(
        _ id: String,
        title: String,
        count: String?,
        queue: MacTransferLine.Queue?,
        share: Double?
    ) -> ShotScene {
        component(id, width: 332, height: 80) {
            MacTransferLine(
                title: title,
                count: count,
                queue: queue,
                bar: share.map {
                    LatestStream.activityBars(holding: ["0": $0])
                        .source(for: "0")
                }
            )
            .transferLineStores()
        }
    }
}
