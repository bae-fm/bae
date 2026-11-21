import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageState`, a release's storage at rest.
    static let macStorageStateScenes: [ShotScene] = [
        storageState("mac.component.storage-state.local", .onOneDevice),
        storageState(
            "mac.component.storage-state.pinned",
            .inStorage(pinned: true)
        ),
        storageState(
            "mac.component.storage-state.in-cloud",
            .inStorage(pinned: false)
        ),
    ]
}
