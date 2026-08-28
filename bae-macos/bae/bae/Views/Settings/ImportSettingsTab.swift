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
                if configStore.config.defaultImportMetadataSource == .findOnline
                {
                    Toggle(
                        "Identify automatically",
                        isOn: automaticIdentification
                    )
                }
            } header: {
                Text("Metadata")
            } footer: {
                if configStore.config.defaultImportMetadataSource == .findOnline
                {
                    Text(
                        "Automatically reads cover text, barcodes, and disc IDs and searches for a match whenever Find online opens."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
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

    private var automaticIdentification: Binding<Bool> {
        Binding(
            get: { configStore.config.automaticImportIdentification },
            set: { enabled in
                do {
                    try importer.setAutomaticIdentification(enabled)
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
