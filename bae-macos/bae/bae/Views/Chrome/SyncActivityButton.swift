import BaeKit
import SwiftUI

/// Sync's place in the title bar: a small spinner while bytes move, a
/// warning symbol while something needs the person, nothing otherwise.
/// Clicking it opens what sync is doing in a popover.
struct SyncActivityButton: View {
    @Environment(SyncActivityStore.self)
    private var store
    @Environment(SettingsNavigation.self)
    private var settingsNavigation
    @Environment(\.openSettingsWindow)
    private var openSettings

    @State
    private var listShown = false

    var body: some View {
        if store.visible {
            Button {
                listShown = true
            } label: {
                SyncActivityGlyph(
                    moving: store.activity?.moving == true,
                    needsAttention: store.activity?.needsAttention == true
                        || store.error != nil
                )
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
            }
            .buttonStyle(IconHoverButtonStyle())
            .help(localizedCoreString("core.activity.title"))
            .accessibilityLabel(localizedCoreString("core.activity.title"))
            .popover(isPresented: $listShown, arrowEdge: .bottom) {
                Form {
                    SyncActivityList(onOpenSettings: openSyncSettings)
                }
                .formStyle(.grouped)
                .frame(width: 360)
                .frame(maxHeight: 480)
            }
        }
    }

    private func openSyncSettings() {
        listShown = false
        settingsNavigation.open(.library, present: { openSettings() })
    }
}

/// The spinner, the warning, or both.
private struct SyncActivityGlyph: View {
    let moving: Bool
    let needsAttention: Bool

    var body: some View {
        HStack(spacing: ThemeSpace.inline) {
            if moving {
                ProgressView()
                    .controlSize(.small)
            }
            if needsAttention {
                Image(systemName: "exclamationmark.triangle.fill")
                    .themeIcon(.small)
                    .foregroundStyle(StatusTone.warning.color)
            }
        }
    }
}

#if DEBUG
    #Preview("Sync activity: moving and needing attention") {
        SyncActivityButton()
            .environment(SyncActivityStore.preview())
            .environment(SettingsNavigation())
            .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
            .padding()
    }

    #Preview("Sync activity: moving") {
        SyncActivityButton()
            .environment(SyncActivityStore.preview(needsAttention: false))
            .environment(SettingsNavigation())
            .environment(\.openSettingsWindow, OpenSettingsWindowAction {})
            .padding()
    }

    #Preview("Sync activity list") {
        Form {
            SyncActivityList(onOpenSettings: {})
        }
        .formStyle(.grouped)
        .frame(width: 360, height: 480)
        .environment(SyncActivityStore.preview())
    }
#endif
