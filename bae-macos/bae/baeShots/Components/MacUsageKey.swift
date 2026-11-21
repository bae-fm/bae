import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacUsageKey`, one kind of the usage legend, in each colour core's
    /// kinds take. The component's fifth row, a kind the legend has no
    /// colour for, has no kind in the app: the app's four kinds are the
    /// rows above.
    static let macUsageKeyScenes: [ShotScene] = [
        usageKey("mac.component.usage-key.pinned", .pinned, "6.1 GB"),
        usageKey("mac.component.usage-key.cache", .cache, "1.8 GB"),
        usageKey("mac.component.usage-key.library", .library, "420 MB"),
        usageKey(
            "mac.component.usage-key.from-before-the-reset",
            .beforeReset,
            "960 MB"
        ),
    ]

    private static func usageKey(
        _ state: String,
        _ kind: BridgeStorageKind,
        _ size: String
    ) -> ShotScene {
        component(state, height: 40) {
            MacUsageKey(color: kind.color, title: kind.title, size: size)
        }
    }
}
