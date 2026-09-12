import BaeKit
import SwiftUI

/// How many files move to and from cloud storage at once. Device-local: a
/// concurrency limit reflects one machine's link and CPU, so unlike most
/// preferences it does not follow the user to another device.
struct TransfersSettingsTab: View {
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
    #Preview("Transfers Settings") {
        TransfersSettingsTab()
            .environment(PreviewData.configStore())
            .environment(Downloads.stub())
            .environment(Sync.stub())
            .environment(UiStore())
            .frame(width: 500, height: 300)
    }
#endif
