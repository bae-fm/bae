import BaeKit
import SwiftUI

/// The panes, in the order the sidebar lists them (`allCases`): what the app
/// looks like, then the library it holds, then playing from it, then the
/// files that move to and from storage, then what this Mac keeps of them,
/// then getting releases in and out, then the servers and automation around
/// it, then the app itself.
enum SettingsTab: Hashable, CaseIterable {
    case appearance, library, playback, transfers, storage, importing, formats,
        casting, subsonic, automation, about

    var title: LocalizedStringKey {
        switch self {
        case .appearance: "Appearance"
        case .library: "Library"
        case .playback: "Playback"
        case .importing: "Import"
        case .formats: "Save as"
        case .transfers: "Transfers"
        case .storage: "Storage"
        case .casting: "Casting"
        case .subsonic: "Subsonic"
        case .automation: "Automation"
        case .about: "About"
        }
    }
}

@MainActor
@Observable
final class SettingsNavigation {
    var selectedTab: SettingsTab = .appearance

    func open(_ tab: SettingsTab, present: () -> Void) {
        selectedTab = tab
        present()
    }
}

/// The settings window's frame: the sidebar down the side and `pane` beside
/// it. The settings window and the screenshots of it both draw this, so a
/// shot shows the window the app opens.
struct SettingsWindowLayout<Pane: View>: View {
    @Binding
    var selection: SettingsTab
    let pane: Pane

    init(
        selection: Binding<SettingsTab>,
        @ViewBuilder pane: () -> Pane
    ) {
        _selection = selection
        self.pane = pane()
    }

    var body: some View {
        // No title bar and no sidebar toggle: the window's buttons sit in the
        // sidebar's top, and each pane puts its own title at its top left.
        HStack(spacing: 0) {
            MacSettingsSidebar(selection: $selection)
            pane
                .scrollContentBackground(.hidden)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .windowBackground()
        }
        .windowBackground()
        .settingsWindowButtonsInSidebar()
        .ignoresSafeArea(.container, edges: .top)
        // The sidebar's width plus the width every pane was drawn against.
        .frame(width: 690, height: 600)
    }
}

/// The settings window: the panes down the side, the chosen one beside them.
///
/// A sidebar and not the toolbar strip a settings window usually has, because
/// eleven panes do not fit in one: the strip put whatever ran past the window's
/// edge into an overflow menu that renders its entries disabled, so the last
/// panes could be seen and not reached. Widening the window until eleven fit is
/// no answer either — the titles are translated, and "Automation" is
/// "การทำงานอัตโนมัติ" in Thai — so the width that fits English hides a pane
/// somewhere else. A list has no edge to run past: it scrolls, and every pane
/// stays reachable in every language however many there come to be.
struct MacSettingsWindow: View {
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
        SettingsWindowLayout(selection: $navigation.selectedTab) {
            pane(for: navigation.selectedTab)
        }
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
        case .storage:
            StorageSettingsTab()
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
