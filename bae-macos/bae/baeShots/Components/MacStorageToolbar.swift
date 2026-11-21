import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageToolbar`, in the window's title bar.
    static let macStorageToolbarScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.storage-toolbar.default",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar()
        },
        ShotScene(
            id: "mac.component.storage-toolbar.inspector-on",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(inspector: true)
        },
        ShotScene(
            id: "mac.component.storage-toolbar.filtered",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(filter: .remote, search: "harbor")
        },
        ShotScene(
            id: "mac.component.storage-toolbar.no-library",
            size: storageToolbarSize,
            frame: .window(title: "Storage Manager")
        ) {
            storageToolbar(off: true)
        },
    ]

    /// The Storage Manager's toolbar, which only a window draws: an empty
    /// window of the toolbar's own.
    @MainActor
    private static func storageToolbar(
        filter: BridgeStorageFilter = .all,
        search: String = "",
        inspector: Bool = false,
        off: Bool = false
    ) -> ShotStage {
        ShotStage(
            Color.clear
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .modifier(
                    MacStorageToolbar(
                        filter: .constant(filter),
                        search: .constant(search),
                        inspectorPresented: .constant(inspector),
                        off: off
                    )
                )
        )
    }

    private static let storageToolbarSize = CGSize(width: 900, height: 120)
}
