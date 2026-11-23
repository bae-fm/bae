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
    /// The view of a Storage Manager board's window, which the board draws
    /// 1,100 by 660 points with its 52-point title bar and toolbar inside;
    /// the shot's window adds that title bar to the view's size.
    static let storageManagerSize = CGSize(width: 1_100, height: 608)

    /// The Storage Manager window (`mac.storage.window*`) with no library
    /// open: its toolbar controls each off, over the message.
    static let storageManagerScenes: [ShotScene] = [
        ShotScene(
            id: "mac.storage.window.no-library",
            size: storageManagerSize,
            frame: .window(title: "Storage Manager", titleBar: .themed)
        ) {
            ShotStage(StorageManagerNoLibrary())
        }
    ]
}
