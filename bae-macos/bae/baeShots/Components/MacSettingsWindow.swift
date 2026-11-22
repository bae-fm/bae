import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSettingsWindow`'s frame: the Library pane, a page pushed over it,
    /// and the Library pane while sync is offline, which marks its row.
    static let macSettingsWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.settings-window.a-pane",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(page: libraryPane())
            )
        },
        ShotScene(
            id: "mac.component.settings-window.a-page-in-a-pane",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(
                    page: syncSettingsPagePreview(
                        LibrarySettingsTabPreviewFixture(
                            storage: PreviewData.previewDropboxStorage
                        ),
                        shown: .fixtureSynced
                    )
                )
            )
        },
        ShotScene(
            id: "mac.component.settings-window.a-pane-with-a-status",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(shown: .fixtureOffline, page: libraryPane())
            )
        },
        ShotScene(
            id: "mac.component.settings-window.in-front",
            size: settingsWindowSize,
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            ShotStage(
                SettingsWindowShot(page: libraryPane())
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
                    page: libraryPane()
                )
            )
        },
    ]

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
