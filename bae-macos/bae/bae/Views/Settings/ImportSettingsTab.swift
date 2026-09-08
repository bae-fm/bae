import BaeKit
import SwiftUI

/// Import metadata defaults and device-local transfer concurrency. Every
/// control writes through core; the config value stream redraws the stored
/// value.
struct ImportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Sync.self)
    private var sync
    @Environment(Importer.self)
    private var importer
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Form {
            Section {
                Picker(
                    "Default metadata source",
                    selection: defaultMetadataSource
                ) {
                    Text("Find online")
                        .tag(BridgeDefaultImportMetadataSource.findOnline)
                    Text(coreString("ui.import.metadata.file_tags"))
                        .tag(BridgeDefaultImportMetadataSource.fileTags)
                    Text("None")
                        .tag(BridgeDefaultImportMetadataSource.none)
                }
            } header: {
                Text("Metadata")
            }

            Section {
                Toggle("Identify automatically", isOn: identifyAutomatically)
                ForEach(configStore.config.metadataSources, id: \.source) {
                    setting in
                    sourceToggle(setting)
                }
            } header: {
                Text("Online lookup")
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text(
                        "New candidates are identified as they are discovered, and when Find online opens. When off, the Identify link starts a run."
                    )
                    Text(
                        "Find online asks the sources that are checked here. The same checkboxes are on the Find online header."
                    )
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                control(
                    label: "Simultaneous uploads",
                    value: configStore.config.maxConcurrentUploads,
                    setValue: sync.setMaxConcurrentUploads
                )
                control(
                    label: "Simultaneous downloads",
                    value: configStore.config.maxConcurrentDownloads,
                    setValue: downloads.setMaxConcurrentDownloads
                )
            } header: {
                Text("Transfers")
            } footer: {
                Text(
                    "How many files upload to cloud storage at once after an import, and how many download at once when a release is pinned."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }

    /// One source's checkbox. Whether it can be moved is core's answer, not
    /// this view's: a source with no credential and the only source left being
    /// asked are both writes core would turn down.
    private func sourceToggle(
        _ setting: BridgeMetadataSourceSetting
    ) -> some View {
        Toggle(
            String(
                localized:
                    "Search \(bridgeMetadataSourceName(source: setting.source))"
            ),
            isOn: Binding(
                get: { setting.availability == .on },
                set: { enabled in
                    do {
                        try importer.setMetadataSourceEnabled(
                            setting.source,
                            enabled
                        )
                    }
                    catch {
                        uiStore.showError(error)
                    }
                }
            )
        )
        .disabled(!setting.canChange)
    }

    private var identifyAutomatically: Binding<Bool> {
        Binding(
            get: { configStore.config.identifyAutomatically },
            set: { enabled in
                do {
                    try importer.setIdentifyAutomatically(enabled)
                }
                catch {
                    uiStore.showError(error)
                }
            }
        )
    }

    private var defaultMetadataSource:
        Binding<BridgeDefaultImportMetadataSource>
    {
        Binding(
            get: { configStore.config.defaultImportMetadataSource },
            set: { source in
                do {
                    try importer.setDefaultMetadataSource(source)
                }
                catch {
                    uiStore.showError(error)
                }
            }
        )
    }

    private func control(
        label: LocalizedStringKey,
        value: UInt32,
        setValue: @escaping @Sendable (UInt32) throws -> Void
    ) -> some View {
        LabeledContent(label) {
            TransferConcurrencyPicker(
                title: label,
                value: value,
                setValue: setValue,
                showError: { uiStore.showError($0) }
            )
            .labelsHidden()
            .fixedSize()
        }
    }
}

#if DEBUG
    #Preview("Import Settings") {
        ImportSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Downloads.stub())
            .environment(Sync.stub())
            .environment(PreviewData.importTabImporter())
            .environment(UiStore())
            .frame(width: 500, height: 500)
    }
#endif
