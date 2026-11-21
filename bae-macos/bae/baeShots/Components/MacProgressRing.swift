import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacProgressRing` in the tones the Storage Manager draws: a file
    /// moving to the cloud and a file pinned to this Mac.
    static let macProgressRingScenes: [ShotScene] = [
        progressRing("mac.component.progress-ring.activity", kind: .upload),
        progressRing("mac.component.progress-ring.pinning", kind: .pin),
    ]

    private static func progressRing(
        _ id: String,
        kind: StorageTransferKind
    ) -> ShotScene {
        component(id, height: 40) {
            MacProgressRing(
                color: kind.color,
                source: OutboxStore(
                    uploadsPaused: false,
                    fileShares: .fileShares(holding: ["file": 0.4])
                )
                .fileShare(forFile: "file")
            )
            .frame(width: 16, height: 16)
        }
    }
}
