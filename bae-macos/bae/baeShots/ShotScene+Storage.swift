import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Storage pane's boards (`mac.settings.storage*`): what this Mac
    /// keeps, with data a reset left behind, and the alert that asks before
    /// deleting it.
    static let storageSettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.storage",
            size: CGSize(width: 500, height: 700)
        ) {
            let store = DeviceStorageStore.staging(.previewUsual)
            return ShotStage(
                storageSettingsPreview(store)
                    .frame(width: 500, height: 700)
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.from-before-reset",
            size: CGSize(width: 500, height: 820)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                storageSettingsPreview(store)
                    .frame(width: 500, height: 820)
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.delete-from-before-reset",
            size: CGSize(width: 500, height: 820)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                storageSettingsPreview(store)
                    .frame(width: 500, height: 820)
                    .overlay {
                        Color.black.opacity(0.3)
                        NSAlertShot {
                            StorageAlerts.deleteBeforeReset(
                                size: UInt64(960_000_000).storageSizeText
                            )
                        }
                    }
            ) { store.storage != nil }
        },
    ]
}
