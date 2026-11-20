import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac windows as components: the main window around the albums, the
    /// welcome window and the Settings window, each in its own window frame,
    /// with the Settings window's sidebar and heading, and the components of
    /// the welcome window, the title bar and the setup sheets.
    static let windowComponentScenes: [ShotScene] =
        mainWindowScenes + welcomeWindowScenes + settingsWindowScenes
        + settingsPartScenes + chromeComponentScenes + welcomeComponentScenes
        + setupComponentScenes

    /// `MacMainWindow` around the library's albums, or the Import section,
    /// with "Low Tide Hours" playing 1:42 into 5:03 or nothing played, and
    /// nothing in the Activity button.
    private static let mainWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.main-window.library",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.import",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .importing, playing: true)
        },
        ShotScene(
            id: "mac.component.main-window.nothing-played",
            size: CGSize(width: 1100, height: 520),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            mainWindow(section: .library, playing: false)
        },
    ]

    /// The main window on `section`, 1100 by 520 as the preview draws it.
    @MainActor
    private static func mainWindow(
        section: MainSection,
        playing: Bool
    ) -> ShotStage {
        let uiStore = storeShowing(section)
        let backing = boardLibraryBacking(uiStore: uiStore)
        return ShotStage(
            ShotScenes.libraryWindow(
                uiStore: uiStore,
                backing: backing,
                needsAttention: false,
                syncActivity: SyncActivityStore(),
                playbackStore: playing ? boardPlayback() : PlaybackStore(),
                size: CGSize(width: 1100, height: 520)
            )
        ) {
            (backing.session.albums.list?.positionCount ?? 0) > 0
        }
    }

    /// `MacWelcomeWindow`: the window's chrome with nothing in it, 600 by
    /// 260 as the preview draws it.
    private static let welcomeWindowScenes: [ShotScene] = [
        ShotScene(
            id: "mac.component.welcome-window.window",
            size: CGSize(width: 600, height: 260),
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            ShotStage(
                MacWelcomeWindow(size: CGSize(width: 600, height: 260)) {
                    Color.clear
                }
            )
        }
    ]

    /// `MacSettingsWindow`'s frame: the Library pane, a page pushed over it,
    /// and the Library pane while sync is offline, which marks its row.
    private static let settingsWindowScenes: [ShotScene] = [
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
    ]

    /// `MacSettingsSidebar` and `MacPageHeading`, on their own.
    private static let settingsPartScenes: [ShotScene] = [
        sidebar(
            "mac.component.settings-sidebar.open-pane",
            tab: .library,
            shown: .fixtureSynced
        ),
        sidebar(
            "mac.component.settings-sidebar.with-a-status",
            tab: .library,
            shown: .fixtureOffline
        ),
        sidebar(
            "mac.component.settings-sidebar.status-on-another-pane",
            tab: .appearance,
            shown: .fixtureOffline
        ),
        component(
            "mac.component.page-heading.pane",
            width: 500,
            height: 70,
            inset: 0
        ) {
            MacPageHeading(title: Text("Library"))
        },
        component(
            "mac.component.page-heading.page-in-a-pane",
            width: 500,
            height: 70,
            inset: 0
        ) {
            MacPageHeading(
                title: Text("Sync"),
                back: .init(title: "Library", action: {})
            )
        },
    ]

    private static func sidebar(
        _ id: String,
        tab: SettingsTab,
        shown: BridgeSyncShown
    ) -> ShotScene {
        component(id, width: 220, height: 400, inset: 0) {
            SettingsSidebarShot(tab: tab, shown: shown)
        }
    }

    private static let settingsWindowSize = CGSize(width: 690, height: 600)

    /// The Library pane as the window opens it: the tab over a library
    /// whose storage is Dropbox.
    @MainActor
    private static func libraryPane() -> some View {
        let fixture = LibrarySettingsTabPreviewFixture(
            storage: PreviewData.previewDropboxStorage
        )
        return LibrarySettingsTab(
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
    }
}

/// The Settings window's sidebar on `tab`, with the sync state its Library
/// row's chip reads.
private struct SettingsSidebarShot: View {
    let shown: BridgeSyncShown

    @State
    private var navigation = SettingsNavigation()

    init(tab: SettingsTab, shown: BridgeSyncShown) {
        self.shown = shown
        let navigation = SettingsNavigation()
        navigation.selectedTab = tab
        _navigation = State(initialValue: navigation)
    }

    var body: some View {
        MacSettingsSidebar(navigation: navigation)
            .environment(DownloadStore())
            .environment(OutboxStore(uploadsPaused: false))
            .environment(SubsonicStore())
            .environment(
                SyncStatusStore(
                    snapshot: BridgeSyncStatusSnapshot(
                        shown: shown,
                        setupRunning: false,
                        undoneSetup: nil
                    )
                )
            )
            .windowBackground()
    }
}
