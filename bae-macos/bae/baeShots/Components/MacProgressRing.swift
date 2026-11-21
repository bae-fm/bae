import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacProgressRing`, which draws a file moving to the cloud and a file
    /// pinned to this Mac alike, in the accent.
    static let macProgressRingScenes: [ShotScene] = [
        component("mac.component.progress-ring.progress", height: 40) {
            MacProgressRing(
                source: OutboxStore(
                    uploadsPaused: false,
                    fileShares: .fileShares(holding: ["file": 0.4])
                )
                .fileShare(forFile: "file")
            )
            .frame(width: 16, height: 16)
        }
    ]
}
