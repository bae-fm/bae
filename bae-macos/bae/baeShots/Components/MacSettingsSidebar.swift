import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSettingsSidebar`, one shot for each row of its preview.
    static let macSettingsSidebarScenes: [ShotScene] = [
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
    ]

    private static func sidebar(
        _ id: String,
        tab: SettingsTab,
        shown: BridgeSyncShown
    ) -> ShotScene {
        component(id, width: 220, height: 400) {
            SettingsSidebarShot(tab: tab, shown: shown)
        }
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
                        hasCloudHome: true,
                        undoneSetup: nil
                    )
                )
            )
            .windowBackground()
    }
}
