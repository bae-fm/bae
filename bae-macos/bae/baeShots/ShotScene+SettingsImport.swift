import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Import settings with no Discogs key and automatic import off.
    static let importSettingsScene = ShotScene(
        id: "mac.settings.import",
        size: CGSize(width: 690, height: 600),
        frame: .window(title: "Settings", titleBar: .hidden)
    ) {
        ShotStage(
            SettingsWindowShot(
                tab: .importing,
                page: ImportSettingsTab()
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: false,
                            discogs: .off,
                            storage: PreviewData.previewStorage
                        )
                    )
                    .environment(Discogs.stub())
                    .environment(Importer.stub())
                    .environment(UiStore())
            )
        )
    }
}
