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
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewUsual)
            return ShotStage(
                SettingsWindowShot(
                    tab: .storage,
                    page: storageSettingsPreview(store)
                )
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.from-before-reset",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                SettingsWindowShot(
                    tab: .storage,
                    page: storageSettingsPreview(store)
                )
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.delete-from-before-reset",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                OverWindow.alert(
                    StorageAlerts.deleteBeforeReset(
                        size: UInt64(960_000_000).fileSizeText
                    ),
                    over: SettingsWindowShot(
                        tab: .storage,
                        page: storageSettingsPreview(store)
                    )
                )
            ) { store.storage != nil }
        },
    ]
}
