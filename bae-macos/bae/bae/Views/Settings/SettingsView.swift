import SwiftUI

/// The panes, in the order the sidebar lists them (`allCases`): what the app
/// looks like, then the library it holds, then playing from it, then getting
/// releases in and out, then the servers and automation around it, then the
/// app itself.
enum SettingsTab: Hashable, CaseIterable {
    case appearance, library, playback, importing, formats, transfers, casting,
        subsonic, automation, about

    var title: LocalizedStringKey {
        switch self {
        case .appearance: "Appearance"
        case .library: "Library"
        case .playback: "Playback"
        case .importing: "Import"
        case .formats: "Formats"
        case .transfers: "Transfers"
        case .casting: "Casting"
        case .subsonic: "Subsonic"
        case .automation: "Automation"
        case .about: "About"
        }
    }

    var symbol: String {
        switch self {
        case .appearance: "paintpalette"
        case .library: "books.vertical"
        case .playback: "play.circle"
        case .importing: "square.and.arrow.down"
        case .formats: "square.and.arrow.up"
        case .transfers: "arrow.up.arrow.down"
        case .casting: "hifispeaker"
        case .subsonic: "dot.radiowaves.left.and.right"
        case .automation: "terminal"
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
/// ten panes do not fit in one: the strip put whatever ran past the window's
/// edge into an overflow menu that renders its entries disabled, so the last
/// panes could be seen and not reached. Widening the window until ten fit is
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
        case .appearance:
            AppearanceSettingsTab()
        case .library:
            LibrarySettingsTab(onForgetLibrary: onForgetLibrary)
        case .playback:
            PlaybackSettingsTab()
        case .importing:
            ImportSettingsTab()
        case .formats:
            FormatsSettingsTab()
        case .transfers:
            TransfersSettingsTab()
        case .casting:
            CastingSettingsTab()
        case .subsonic:
            SubsonicSettingsTab()
        case .automation:
            AutomationSettingsTab()
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
