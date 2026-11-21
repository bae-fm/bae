import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageUsage`, with and without data a reset left, and with
    /// nothing pinned.
    static let macStorageUsageScenes: [ShotScene] = [
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

    private static func storageUsage(
        _ id: String,
        _ storage: BridgeDeviceStorage,
        height: CGFloat
    ) -> ShotScene {
        component(id, width: 580, height: height) {
            MacStorageUsage(usage: StorageUsage(storage))
        }
    }
}
