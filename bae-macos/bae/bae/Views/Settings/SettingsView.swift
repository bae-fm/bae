import SwiftUI

enum SettingsTab: Hashable, CaseIterable {
    case library, playback, importing, casting, formats, automation, subsonic,
        discogs, about

    var title: LocalizedStringKey {
        switch self {
        case .library: "Library"
        case .playback: "Playback"
        case .importing: "Import"
        case .casting: "Casting"
        case .formats: "Formats"
        case .automation: "Automation"
        case .subsonic: "Subsonic"
        case .discogs: "Discogs"
        case .about: "About"
        }
    }

    var symbol: String {
        switch self {
        case .library: "books.vertical"
        case .playback: "play.circle"
        case .importing: "square.and.arrow.down"
        case .casting: "hifispeaker"
        case .formats: "square.and.arrow.up"
        case .automation: "terminal"
        case .subsonic: "dot.radiowaves.left.and.right"
        case .discogs: "network"
        case .about: "info.circle"
        }
    }
}

@MainActor
@Observable
final class SettingsNavigation {
    var selectedTab: SettingsTab = .library

    func open(_ tab: SettingsTab, present: () -> Void) {
        selectedTab = tab
        present()
    }
}

/// The settings window: the panes down the side, the chosen one beside them.
///
/// A sidebar and not the toolbar strip a settings window usually has, because
/// nine panes do not fit in one: the strip put whatever ran past the window's
/// edge into an overflow menu that renders its entries disabled, so the last
/// panes could be seen and not reached. Widening the window until nine fit is
/// no answer either — the titles are translated, and "Automation" is
/// "การทำงานอัตโนมัติ" in Thai — so the width that fits English hides a pane
/// somewhere else. A list has no edge to run past: it scrolls, and every pane
/// stays reachable in every language however many there come to be.
struct SettingsView: View {
    let checkForUpdatesViewModel: CheckForUpdatesViewModel
    let onForgetLibrary: () -> Void

    @Environment(SettingsNavigation.self)
    private var navigation

    var body: some View {
        @Bindable
        var navigation = navigation
        NavigationSplitView(columnVisibility: .constant(.all)) {
            List(
                SettingsTab.allCases,
                id: \.self,
                selection: $navigation.selectedTab
            ) {
                tab in
                Label(tab.title, systemImage: tab.symbol)
                    .tag(tab)
            }
            .navigationSplitViewColumnWidth(190)
        } detail: {
            pane(for: navigation.selectedTab)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .toolbar(removing: .sidebarToggle)
        // The sidebar's width plus the width every pane was drawn against.
        .frame(width: 690, height: 600)
    }

    @ViewBuilder
    private func pane(for selectedTab: SettingsTab) -> some View {
        switch selectedTab {
        case .library:
            LibrarySettingsTab(onForgetLibrary: onForgetLibrary)
        case .playback:
            PlaybackSettingsTab()
        case .importing:
            ImportSettingsTab()
        case .casting:
            CastingSettingsTab()
        case .formats:
            FormatsSettingsTab()
        case .automation:
            AutomationSettingsTab()
        case .subsonic:
            SubsonicSettingsTab()
        case .discogs:
            DiscogsSettingsTab()
        case .about:
            AboutSettingsTab(
                canCheckForUpdates: checkForUpdatesViewModel.canCheckForUpdates,
                onCheckForUpdates: {
                    checkForUpdatesViewModel.checkForUpdates()
                },
            )
        }
    }
}
