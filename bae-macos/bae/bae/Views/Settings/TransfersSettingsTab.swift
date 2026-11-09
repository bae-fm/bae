import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("TransfersSettings")

/// Which transfers the person paused, as the Settings sidebar flags them.
enum PausedTransfers {
    case uploads, downloads, both

    /// What is paused, or nil when nothing is.
    init?(uploads: Bool, downloads: Bool) {
        switch (uploads, downloads) {
        case (true, true): self = .both
        case (true, false): self = .uploads
        case (false, true): self = .downloads
        case (false, false): return nil
        }
    }

    var arrow: String {
        switch self {
        case .uploads: "\u{2191}"
        case .downloads: "\u{2193}"
        case .both: "\u{21C5}"
        }
    }

    var accessibilityLabel: LocalizedStringKey {
        switch self {
        case .uploads: "Uploads paused"
        case .downloads: "Downloads paused"
        case .both: "Uploads and downloads paused"
        }
    }
}

/// Pausing the transfers this device makes, and how many files move to and
/// from cloud storage at once, set per device.
struct TransfersSettingsTab: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(DownloadStore.self)
    private var downloadStore
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Form {
            Section {
                Toggle(
                    "Pause uploads",
                    isOn: pauseBinding(
                        paused: outboxStore.uploadsPaused,
                        set: sync.setSyncPaused,
                        failure: "upload pause"
                    )
                )
                Toggle(
                    "Pause downloads",
                    isOn: pauseBinding(
                        paused: downloadStore.downloadsPaused,
                        set: downloads.setDownloadsPaused,
                        failure: "download pause"
                    )
                )
            } footer: {
                Text(
                    "Paused files keep their place and continue when you resume. Changes to the library still sync."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                control(
                    label: "Uploads at once",
                    value: configStore.config.maxConcurrentUploads,
                    setValue: sync.setMaxConcurrentUploads
                )
                control(
                    label: "Downloads at once",
                    value: configStore.config.maxConcurrentDownloads,
                    setValue: downloads.setMaxConcurrentDownloads
                )
            } footer: {
                Text(
                    "How many files upload to storage at once after an import, and how many download at once. Downloads are pinning and making local; they share this limit and Pause downloads."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }

    /// A switch showing `paused` that asks for the pause it is turned to.
    private func pauseBinding(
        paused: Bool,
        set: @escaping @Sendable (Bool) async throws -> Void,
        failure: String
    ) -> Binding<Bool> {
        Binding(
            get: { paused },
            set: { paused in
                Task {
                    do { try await set(paused) }
                    catch {
                        logger.error(
                            "Failed to set \(failure): \(String(reflecting: error))"
                        )
                        uiStore.showError(error)
                    }
                }
            }
        )
    }

    private func control(
        label: LocalizedStringKey,
        value: UInt32,
        setValue: @escaping @Sendable (UInt32) async throws -> Void
    ) -> some View {
        LabeledContent(label) {
            TransferConcurrencyPicker(
                title: label,
                value: value,
                limit: configStore.config.maxConcurrentTransfers,
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
            .environment(DownloadStore())
            .environment(OutboxStore(uploadsPaused: false))
            .environment(Sync.stub())
            .environment(UiStore())
            .frame(width: 500, height: 420)
    }
#endif
