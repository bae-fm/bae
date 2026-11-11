import BaeKit
import SwiftUI

/// The panes, in the order the sidebar lists them (`allCases`): what the app
/// looks like, then the library it holds, then playing from it, then the
/// files that move to and from storage, then getting releases in and out, then
/// the servers and automation around it, then the app itself.
enum SettingsTab: Hashable, CaseIterable {
    case appearance, library, playback, transfers, importing, formats, casting,
        subsonic, automation, about

    var title: LocalizedStringKey {
        switch self {
        case .appearance: "Appearance"
        case .library: "Library"
        case .playback: "Playback"
        case .importing: "Import"
        case .formats: "Save as"
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

/// The amber Paused beside Transfers in the sidebar, with the arrows for
/// which transfers are paused.
private struct PausedBadge: View {
    let paused: PausedTransfers

    var body: some View {
        HStack(spacing: 2) {
            Text(verbatim: paused.arrow)
            Text("Paused")
        }
        .themeText(.detail)
        .foregroundStyle(Theme.warning)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(paused.accessibilityLabel)
    }
}

/// The sync state beside Library in the sidebar, in core's words and the color
/// it gives them, shown only while the state is amber or red.
private struct SyncBadge: View {
    let shown: BridgeSyncShown

    var body: some View {
        Text(shown.statusLine)
            .themeText(.detail)
            .foregroundStyle(shown.statusTone.color)
    }
}

/// Offline beside Subsonic in the sidebar, while the server is switched on and
/// not running: a warning, or danger when it could not start.
private struct SubsonicBadge: View {
    let tone: StatusTone

    var body: some View {
        Text("Offline")
            .themeText(.detail)
            .foregroundStyle(tone.color)
    }
}

/// The panes down the side of the settings window, with the state some of them
/// carry beside their title.
struct SettingsSidebar: View {
    @Binding
    var selection: SettingsTab

    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(SyncStatusStore.self)
    private var syncStatus
    @Environment(SubsonicStore.self)
    private var subsonicStore

    var body: some View {
        List(SettingsTab.allCases, id: \.self, selection: $selection) { tab in
            HStack {
                Label(tab.title, systemImage: tab.symbol)
                if tab == .transfers,
                    let paused = PausedTransfers(
                        uploads: outboxStore.uploadsPaused,
                        downloads: downloadStore.downloadsPaused
                    )
                {
                    Spacer()
                    PausedBadge(paused: paused)
                }
                if tab == .library, let shown = troubledSync {
                    Spacer()
                    SyncBadge(shown: shown)
                }
                if tab == .subsonic,
                    case .notRunning(_, let tone) = subsonicStore.view.status
                {
                    Spacer()
                    SubsonicBadge(tone: tone.statusTone)
                }
            }
            .tag(tab)
        }
    }

    /// The sync state, while it is amber or red.
    private var troubledSync: BridgeSyncShown? {
        guard let shown = syncStatus.shown,
            shown.statusTone == .warning || shown.statusTone == .danger
        else { return nil }
        return shown
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
    let onResetDevice: () -> Void
    let onRestoreFromCode: () -> Void
    let onRenameLibrary: () -> Void
    let onRemoveLibrary: () -> Void

    @Environment(SettingsNavigation.self)
    private var navigation

    var body: some View {
        @Bindable
        var navigation = navigation
        NavigationSplitView(columnVisibility: .constant(.all)) {
            SettingsSidebar(selection: $navigation.selectedTab)
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
            LibrarySettingsTab(
                onResetDevice: onResetDevice,
                onRestoreFromCode: onRestoreFromCode,
                onCheckForUpdates: {
                    checkForUpdatesViewModel.checkForUpdates()
                },
                onRenameLibrary: onRenameLibrary,
                onRemoveLibrary: onRemoveLibrary
            )
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
