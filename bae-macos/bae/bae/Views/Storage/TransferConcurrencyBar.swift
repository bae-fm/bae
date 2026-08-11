import BaeKit
import SwiftUI

/// Always-visible bottom bar with the two device-local transfer-concurrency
/// settings: how many downloads a pin fetches at once and how many uploads the
/// sync drain runs at once. It sits outside the download and outbox queue panes
/// (which hide when their queues are idle) so the settings stay reachable
/// whether or not a transfer is in flight.
struct TransferConcurrencyBar: View {
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(Downloads.self)
    private var downloads
    @Environment(Sync.self)
    private var sync
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        HStack(spacing: 32) {
            control(
                label: "Simultaneous downloads",
                value: configStore.config.maxConcurrentDownloads,
                setValue: downloads.setMaxConcurrentDownloads
            )
            control(
                label: "Simultaneous uploads",
                value: configStore.config.maxConcurrentUploads,
                setValue: sync.setMaxConcurrentUploads
            )
            Spacer()
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private func control(
        label: LocalizedStringKey,
        value: UInt32,
        setValue: @escaping @Sendable (UInt32) throws -> Void
    ) -> some View {
        HStack(spacing: 8) {
            Text(label)
                .font(.callout)
                .foregroundStyle(.secondary)
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
    #Preview("Concurrency bar") {
        TransferConcurrencyBar()
            .environment(PreviewData.configStore())
            .environment(Downloads.stub())
            .environment(Sync.stub())
            .environment(UiStore())
            .frame(width: 700)
    }
#endif
