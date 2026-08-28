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
                    "Open unseeded candidates in",
                    selection: defaultMetadataMode
                ) {
                    Text(coreString("ui.import.metadata.lookup"))
                        .tag(BridgeDefaultImportMetadataMode.lookup)
                    Text(coreString("ui.import.metadata.file_tags"))
                        .tag(BridgeDefaultImportMetadataMode.fileTags)
                    Text("Manual").tag(BridgeDefaultImportMetadataMode.manual)
                    Text("Last used")
                        .tag(BridgeDefaultImportMetadataMode.lastUsed)
                }
                Toggle(
                    "Identify Lookup candidates automatically",
                    isOn: automaticMetadataLookup
                )
            } header: {
                Text("Metadata")
            } footer: {
                Text(
                    "Automatic identification reads cover text, barcodes, and disc IDs only while a candidate uses Lookup. File tags and Manual never run it."
                )
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

    private var automaticMetadataLookup: Binding<Bool> {
        Binding(
            get: { configStore.config.automaticImportMetadataLookup },
            set: { enabled in
                do {
                    try importer.setAutomaticMetadataLookup(enabled)
                }
                catch {
                    uiStore.showError(error)
                }
            }
        )
    }

    private var defaultMetadataMode: Binding<BridgeDefaultImportMetadataMode> {
        Binding(
            get: { configStore.config.defaultImportMetadataMode },
            set: { mode in
                do {
                    try importer.setDefaultMetadataMode(mode)
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
