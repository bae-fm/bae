import BaeKit
import SwiftUI

/// The two device-local transfer-concurrency settings: how many uploads the
/// sync drain runs at once and how many downloads a pin fetches at once. Each
/// write lands in the config and on the open store together, so the next
/// drain pass or pin runs under the new limit.
struct ImportSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Form {
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
            .environment(UiStore())
            .frame(width: 500, height: 300)
    }
#endif
