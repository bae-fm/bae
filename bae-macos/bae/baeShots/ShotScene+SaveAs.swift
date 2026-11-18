import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Save as pane's board (`mac.settings.save-as`): the Settings window
    /// on the pane, with its formats, its two name fields and their previews.
    static let saveAsSettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.save-as",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    tab: .formats,
                    page: FormatsSettingsTab()
                        .environment(PreviewData.configStore())
                        .environment(Outputs.stub())
                        .environment(UiStore())
                )
            )
        }
    ]
}
