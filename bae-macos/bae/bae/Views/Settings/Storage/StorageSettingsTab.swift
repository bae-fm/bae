import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("StorageSettings")

/// What this Mac keeps for the library, and what the person can do about it:
/// how much is cached and for how long it stays, whether Unpin asks first,
/// and the data a reset left behind.
struct StorageSettingsTab: View {
    @Environment(DeviceStorageStore.self)
    private var store
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var asker = AlertAsker()

    var body: some View {
        StorageSettingsForm(
            storage: store.storage,
            asksBeforeUnpinning: askBinding,
            onClearCache: { Task { await store.clearCache() } },
            onSetCacheLimit: { choice in
                Task { await store.setCacheLimit(choice) }
            },
            onDeleteBeforeReset: { size in
                Task {
                    guard
                        await StorageAlerts.confirms(
                            StorageAlerts.deleteBeforeReset(size: size),
                            asking: asker
                        )
                    else { return }
                    await store.deleteDataFromBeforeReset()
                }
            }
        )
        .presentsAlerts(of: asker)
        .task { await store.follow() }
    }

    private var askBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.askBeforeUnpinning },
            set: { asks in
                Task {
                    do { try await downloads.setAskBeforeUnpinning(asks) }
                    catch {
                        logger.error(
                            "Failed to set ask before unpinning: \(String(reflecting: error))"
                        )
                        uiStore.showError(error)
                    }
                }
            }
        )
    }
}

/// The Storage pane drawn from values: it reads no app state and does
/// nothing itself, so the pane's states can be previewed and shot.
struct StorageSettingsForm: View {
    /// What the device keeps; nil before core's first value, when only the
    /// Unpin switch shows.
    let storage: BridgeDeviceStorage?
    @Binding
    var asksBeforeUnpinning: Bool
    let onClearCache: () -> Void
    let onSetCacheLimit: (BridgeCacheLimitChoice) -> Void
    /// Asks to delete the data from before the reset, whose size is given.
    let onDeleteBeforeReset: (String) -> Void

    var body: some View {
        SettingsPane(.storage) {
            form
        }
    }

    private var form: some View {
        Form {
            if let storage {
                Section {
                    StorageUsageView(usage: StorageUsage(storage))
                }
                Section("Cache") {
                    LabeledContent {
                        Button("Clear cache", action: onClearCache)
                            .disabled(!storage.canClearCache)
                    } label: {
                        row(
                            "Cache",
                            note:
                                "Tracks you played from the cloud, and covers and pictures. They download again when needed."
                        )
                    }
                    Picker("Keep up to", selection: cacheLimit(storage)) {
                        ForEach(storage.cacheLimits, id: \.choice) { limit in
                            Text(verbatim: limit.bytes.fileSizeText)
                                .tag(limit.choice)
                        }
                    }
                }
            }
            Section {
                SettingToggleRow(
                    title: "Ask before unpinning",
                    note:
                        "Unpinning removes a release’s files from this Mac at once.",
                    isOn: $asksBeforeUnpinning
                )
            } header: {
                Text("Pinned")
            } footer: {
                VStack(alignment: .leading, spacing: ThemeSpace.related) {
                    Text(
                        "Pinned releases stay whole on this Mac to play offline, and do not count toward the cache."
                    )
                    if storage?.beforeReset == nil { localReleasesNote }
                }
            }
            if let beforeReset = storage?.beforeReset {
                Section {
                    LabeledContent {
                        Button(
                            "Delete…",
                            role: .destructive,
                            action: {
                                onDeleteBeforeReset(beforeReset.fileSizeText)
                            }
                        )
                    } label: {
                        row(
                            "Data from before you reset this Mac",
                            note:
                                "The library as it was on this Mac before you reset from cloud. Nothing uses it."
                        )
                    }
                } header: {
                    Text("From before the reset")
                } footer: {
                    localReleasesNote
                }
            }
        }
        .formStyle(.settings)
    }

    private var localReleasesNote: some View {
        Text("Local releases stay in your folders and are not counted here.")
    }

    private func row(
        _ title: LocalizedStringKey,
        note: LocalizedStringKey
    ) -> some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(title)
            Text(note)
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
    }

    private func cacheLimit(
        _ storage: BridgeDeviceStorage
    ) -> Binding<BridgeCacheLimitChoice> {
        Binding(
            get: { storage.cacheLimit.choice },
            set: { onSetCacheLimit($0) }
        )
    }
}

#if DEBUG
    /// The Storage pane over `store`, as every board state draws it.
    @MainActor
    func storageSettingsPreview(_ store: DeviceStorageStore) -> some View {
        StorageSettingsTab()
            .environment(store)
            .environment(PreviewData.configStore())
            .environment(Downloads.stub())
            .environment(UiStore())
    }

    #Preview("Storage Settings") {
        storageSettingsPreview(.staging(.previewUsual))
            .frame(width: 500, height: 700)
    }

    #Preview("Storage Settings, Data From Before A Reset") {
        storageSettingsPreview(.staging(.previewBeforeReset))
            .frame(width: 500, height: 820)
    }

    #Preview("Storage Settings, Nothing Pinned") {
        storageSettingsPreview(.staging(.previewNothingPinned))
            .frame(width: 500, height: 700)
    }

    #Preview("Storage Settings, Deleting Data From Before A Reset") {
        storageSettingsPreview(.staging(.previewBeforeReset))
            .frame(width: 500, height: 820)
            .overlay(alignment: .top) {
                let alert = StorageAlerts.deleteBeforeReset(
                    size: UInt64(960_000_000).fileSizeText
                )
                MacAlertView(alert.0, buttons: alert.1)
                    .elevatedFace()
                    .padding(.top, ThemeSpace.page)
            }
    }
#endif
