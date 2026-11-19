import BaeKit
import SwiftUI

/// The amber Paused chip beside Transfers in the sidebar, with the arrows for
/// which transfers are paused.
private struct PausedBadge: View {
    let paused: PausedTransfers

    var body: some View {
        HStack(spacing: ThemeSpace.line) {
            Text(verbatim: paused.arrow)
            Text("Paused")
        }
        .chipLook(.warning)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(paused.accessibilityLabel)
    }
}

/// The sync state beside Library in the sidebar as a chip, in core's short
/// words and the tone it gives them, shown only while the state is amber or red.
private struct SyncBadge: View {
    let shown: BridgeSyncShown

    var body: some View {
        StatusChip(verbatim: shown.shortStatusLine, tone: shown.statusTone)
    }
}

/// Offline beside Subsonic in the sidebar, while the server is switched on and
/// not running: a warning, or danger when it could not start.
private struct SubsonicBadge: View {
    let tone: StatusTone

    var body: some View {
        StatusChip("Offline", tone: tone)
    }
}

/// The panes down the side of the settings window, with the state some of them
/// carry beside their title.
///
/// The chosen pane is drawn here, in the accent the person chose, and not by a
/// `List`'s selection: macOS draws that in its own accent, which the app can
/// declare once (the AccentColor asset) and cannot change while it runs.
struct MacSettingsSidebar: View {
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
    @Environment(\.accentChoice)
    private var accent

    @FocusState
    private var focused: Bool

    var body: some View {
        ScrollView {
            VStack(spacing: ThemeSpace.line) {
                ForEach(SettingsTab.allCases, id: \.self) { tab in
                    row(tab)
                }
            }
            .padding(.horizontal, ThemeSpace.related)
            .padding(.vertical, ThemeSpace.related)
        }
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(.upArrow) { move(by: -1) }
        .onKeyPress(.downArrow) { move(by: 1) }
        // Room for the window's buttons, which sit in the sidebar's top.
        .padding(.top, MacSettingsSidebar.buttonsRoom)
        .frame(width: MacSettingsSidebar.width)
        .background(
            Theme.surface,
            in: RoundedRectangle(cornerRadius: MacSettingsSidebar.cornerRadius)
        )
        .shadow(color: Theme.shadow, radius: 2, y: 1)
        .padding(.leading, ThemeSpace.related)
        .padding(.vertical, ThemeSpace.related)
    }

    private func row(_ tab: SettingsTab) -> some View {
        let chosen = tab == selection
        return Button {
            selection = tab
        } label: {
            HStack(spacing: ThemeSpace.related) {
                Text(tab.title)
                Spacer(minLength: 0)
                if tab == .transfers,
                    let paused = PausedTransfers(
                        uploads: outboxStore.uploadsPaused,
                        downloads: downloadStore.downloadsPaused
                    )
                {
                    PausedBadge(paused: paused)
                }
                if tab == .library, let shown = troubledSync {
                    SyncBadge(shown: shown)
                }
                if tab == .subsonic,
                    case .notRunning(_, let tone) = subsonicStore.view.status
                {
                    SubsonicBadge(tone: tone.statusTone)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, ThemeSpace.fieldInset)
            .padding(.vertical, ThemeSpace.inline + ThemeSpace.hairline)
            .foregroundStyle(chosen ? Theme.onFill : Theme.textPrimary)
            .environment(\.chipsOnFill, chosen)
            .background(
                chosen ? accent.buttonColor : Color.clear,
                in: RoundedRectangle(cornerRadius: ThemeRadius.field)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    /// Moves the chosen pane `step` panes down the list, stopping at its ends.
    private func move(by step: Int) -> KeyPress.Result {
        let tabs = SettingsTab.allCases
        guard let index = tabs.firstIndex(of: selection) else {
            return .ignored
        }
        let target = min(max(index + step, 0), tabs.count - 1)
        selection = tabs[target]
        return .handled
    }

    private static let width: CGFloat = 190
    private static let buttonsRoom: CGFloat = 24
    private static let cornerRadius: CGFloat = 12

    /// The sync state, while it is amber or red.
    private var troubledSync: BridgeSyncShown? {
        guard let shown = syncStatus.shown,
            shown.statusTone == .warning || shown.statusTone == .danger
        else { return nil }
        return shown
    }
}
