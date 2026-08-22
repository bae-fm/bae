import SwiftUI

enum SettingsTab: Hashable {
    case library, playback, importing, casting, formats, automation, subsonic,
        discogs, about
}

struct SettingsView: View {
    let checkForUpdatesViewModel: CheckForUpdatesViewModel
    /// Remove the active library from this device. Implemented by
    /// AppDelegate: runs the bridge forget, tears down the open service,
    /// and returns the main window to the welcome chooser.
    let onForgetLibrary: () -> Void

    @State
    private var selectedTab: SettingsTab = .library

    var body: some View {
        TabView(selection: $selectedTab) {
            LibrarySettingsTab(onForgetLibrary: onForgetLibrary)
                .tag(SettingsTab.library)
                .tabItem { Label("Library", systemImage: "books.vertical") }
            PlaybackSettingsTab()
                .tag(SettingsTab.playback)
                .tabItem { Label("Playback", systemImage: "play.circle") }
            ImportSettingsTab()
                .tag(SettingsTab.importing)
                .tabItem {
                    Label("Import", systemImage: "square.and.arrow.down")
                }
            CastingSettingsTab()
                .tag(SettingsTab.casting)
                .tabItem { Label("Casting", systemImage: "hifispeaker") }
            FormatsSettingsTab()
                .tag(SettingsTab.formats)
                .tabItem {
                    Label("Formats", systemImage: "square.and.arrow.up")
                }
            AutomationSettingsTab()
                .tag(SettingsTab.automation)
                .tabItem { Label("Automation", systemImage: "terminal") }
            SubsonicSettingsTab()
                .tag(SettingsTab.subsonic)
                .tabItem {
                    Label(
                        "Subsonic",
                        systemImage: "dot.radiowaves.left.and.right"
                    )
                }
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
