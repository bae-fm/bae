import SwiftUI

enum SettingsTab: Hashable {
    case library, playback, export, automation, discogs, about
}

struct SettingsView: View {
    let checkForUpdatesViewModel: CheckForUpdatesViewModel

    @State
    private var selectedTab: SettingsTab = .library

    var body: some View {
        TabView(selection: $selectedTab) {
            LibrarySettingsTab()
                .tag(SettingsTab.library)
                .tabItem { Label("Library", systemImage: "books.vertical") }
            PlaybackSettingsTab()
                .tag(SettingsTab.playback)
                .tabItem { Label("Playback", systemImage: "play.circle") }
            ExportSettingsTab()
                .tag(SettingsTab.export)
                .tabItem {
                    Label("Export", systemImage: "square.and.arrow.up")
                }
            AutomationSettingsTab()
                .tag(SettingsTab.automation)
                .tabItem { Label("Automation", systemImage: "terminal") }
            DiscogsSettingsTab()
                .tag(SettingsTab.discogs)
                .tabItem { Label("Discogs", systemImage: "network") }
            AboutSettingsTab(
                canCheckForUpdates: checkForUpdatesViewModel.canCheckForUpdates,
                onCheckForUpdates: {
                    checkForUpdatesViewModel.checkForUpdates()
                },
            )
            .tag(SettingsTab.about)
            .tabItem { Label("About", systemImage: "info.circle") }
        }
        .frame(width: 500, height: 600)
    }
}
