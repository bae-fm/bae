import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportEmptyState` before a Discogs key is added and after.
    static let macImportEmptyStateScenes: [ShotScene] = [
        importEmptyState(
            "mac.component.import-empty-state.no-discogs-key",
            discogsUsable: false
        ),
        importEmptyState(
            "mac.component.import-empty-state.discogs-key-added",
            discogsUsable: true
        ),
    ]

    private static func importEmptyState(
        _ id: String,
        discogsUsable: Bool
    ) -> ShotScene {
        component(id, width: 560, height: 396) {
            MacImportEmptyState()
                .environment(UiStore())
                .environment(SettingsNavigation())
                .environment(
                    PreviewData.makeConfigStore(
                        libraryFullWidth: false,
                        discogsUsable: discogsUsable
                    )
                )
        }
    }
}
