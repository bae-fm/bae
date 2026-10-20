import BaeKit
import SwiftUI

/// Sync's place in the library's toolbar: a small spinner while bytes move,
/// a warning symbol while something needs the person, nothing otherwise.
/// Tapping it opens what sync is doing in a sheet.
struct SyncActivityToolbarButton: View {
    /// Opens what sync is waiting on, where each record has its action.
    let onOpenSettings: () -> Void
    let onOpenDownloads: () -> Void

    @Environment(SyncActivityStore.self)
    private var store

    @State
    private var listShown = false

    var body: some View {
        if store.visible {
            Button {
                listShown = true
            } label: {
                HStack(spacing: ThemeSpace.inline) {
                    if store.activity?.moving == true {
                        ProgressView()
                            .controlSize(.small)
                    }
                    if store.activity?.needsAttention == true
                        || store.error != nil
                    {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(StatusTone.warning.color)
                    }
                }
            }
            .accessibilityLabel(localizedCoreString("core.activity.title"))
            .sheet(isPresented: $listShown) {
                SyncActivitySheet(
                    onOpenSettings: {
                        listShown = false
                        onOpenSettings()
                    },
                    onOpenDownloads: {
                        listShown = false
                        onOpenDownloads()
                    }
                )
            }
        }
    }
}

/// The sync activity list in its own sheet, closed with Done.
private struct SyncActivitySheet: View {
    let onOpenSettings: () -> Void
    let onOpenDownloads: () -> Void

    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        NavigationStack {
            List {
                SyncActivityList(
                    onOpenSettings: onOpenSettings,
                    onOpenDownloads: onOpenDownloads
                )
            }
            .navigationTitle(localizedCoreString("core.activity.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

#if DEBUG
#Preview("Sync activity sheet") {
    SyncActivitySheet(onOpenSettings: {}, onOpenDownloads: {})
        .environment(SyncActivityStore.preview())
}
#endif
