import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSettingsWindow`'s frame with only the heading of its pane: the
    /// Library pane, a page pushed over it, the Library pane while sync is
    /// offline, which marks its row, and the Save as pane in front.
    static let macSettingsWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.settings-window.a-pane",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(page: pane("Library"))
            )
        },
        ShotScene(
            id: "mac.component.settings-window.a-page-in-a-pane",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(page: pane("Sync", back: "Library"))
            )
        },
        ShotScene(
            id: "mac.component.settings-window.a-pane-with-a-status",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    shown: .fixtureOffline,
                    page: pane("Library")
                )
            )
        },
        ShotScene(
            id: "mac.component.settings-window.in-front",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(tab: .formats, page: pane("Save as"))
            )
        },
        ShotScene(
            id: "mac.component.settings-window.a-pane-with-a-danger-status",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    shown: BridgeSyncShown(
                        status: .offline,
                        statusKey: "core.sync.offline",
                        shortStatusKey: "core.sync.offline",
                        tone: .danger,
                        cause: nil,
                        fix: nil,
                        lastSynced: ShotClock.moment(secondsAgo: 2 * 3_600)
                    ),
                    page: pane("Library")
                )
            )
        },
    ]

    /// A pane with only its heading, as the design's preview draws the
    /// window, and a back button to `back` on a page inside a pane.
    private static func pane(_ title: String, back: String? = nil) -> some View
    {
        MacPageHeading(
            title: Text(verbatim: title),
            back: back.map { MacPageHeading.Back(title: $0, action: {}) }
        )
        .placedInPane()
        .frame(maxHeight: .infinity, alignment: .top)
    }

    private static let settingsWindowSize = CGSize(width: 690, height: 600)

    /// The Library pane as the window opens it: the tab over a library
    /// whose storage is Dropbox.
    @MainActor
    static func libraryPane(
        _ fixture: LibrarySettingsTabPreviewFixture =
            LibrarySettingsTabPreviewFixture(
                storage: PreviewData.previewDropboxStorage
            )
    ) -> some View {
        LibrarySettingsTab(
            onResetDevice: {},
            onRestoreFromCode: {},
            onCheckForUpdates: {},
            onRenameLibrary: {},
            onRemoveLibrary: {}
        )
        .environment(PreviewData.previewSync())
        #if BAE_OAUTH_PROVIDERS
            .environment(CloudSyncSetup())
        #endif
        .environment(fixture.configStore)
        .environment(fixture.membership)
        .environment(UiStore())
        .environment(fixture.libraryName)
        .environment(fixture.ratingSystems)
    }
}
