import BaeKit
import Foundation
import SwiftUI

/// Footer under the storage table: the current filter's release count and
/// total size on the left; on the right, each transfer queue holding work,
/// with how fast it goes and the control that pauses or resumes it whole.
struct StorageFooter: View {
    let list: StorageList
    /// The size of every row the current filter matches; nil, and not shown,
    /// until fetched.
    let totalSize: UInt64?
    /// The transfer queues holding work; nil, and none shown, until core
    /// answers.
    let queues: BridgeStorageQueues?

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Text("\(list.totalCount) releases")
            if let totalSize {
                Text(verbatim: "\u{00B7}")
                Text(
                    "Total: \(ByteCountFormatter.string(fromByteCount: Int64(totalSize), countStyle: .file))"
                )
            }
            Spacer()
            if let uploads = queues?.uploads {
                StorageQueueControl(queue: .uploads, paused: uploads.paused) {
                    StorageUploadRate()
                }
            }
            if let pins = queues?.pins {
                StorageQueueControl(queue: .pins, paused: pins.paused) {
                    if let active = pins.active {
                        StoragePinRate(releaseId: active)
                    }
                }
            }
            if let outputs = queues?.outputs {
                StorageQueueControl(queue: .outputs, paused: outputs.paused) {
                    if let active = outputs.active {
                        StorageOutputPercent(releaseId: active)
                    }
                }
            }
        }
        .foregroundStyle(.secondary)
        .themeText(.body)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}

/// The footer's three queues.
private enum StorageQueueKind {
    case uploads
    case pins
    case outputs
}

/// One running queue in the footer: its glyph, how fast it goes, or how far
/// for exports, which count percent rather than bytes, and the button that
/// pauses it, or resumes it while paused.
private struct StorageQueueControl<Progress: View>: View {
    let queue: StorageQueueKind
    let paused: Bool
    @ViewBuilder
    let progress: () -> Progress
    @Environment(Sync.self)
    private var sync
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.inline) {
                glyph
                    .themeIcon(.badge)
                progress()
                    .monospacedDigit()
            }
            Button(action: setPaused) {
                Image(systemName: paused ? "play.fill" : "pause.fill")
                    .themeIcon(.badge)
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(pauseLabel)
            .help(pauseLabel)
        }
    }

    @ViewBuilder
    private var glyph: some View {
        switch queue {
        case .uploads:
            Image(systemName: "arrow.up")
                .foregroundStyle(StorageTransferKind.upload.color)
                .accessibilityLabel(Text("Uploads"))
        case .pins:
            Image(systemName: "arrow.down")
                .foregroundStyle(StorageTransferKind.pin.color)
                .accessibilityLabel(Text("Pinning"))
        case .outputs:
            Image(systemName: "square.and.arrow.up")
                .accessibilityLabel(Text("Exports"))
        }
    }

    private var pauseLabel: Text {
        switch (queue, paused) {
        case (.uploads, false): Text("Pause uploads")
        case (.uploads, true): Text("Resume uploads")
        case (.pins, false): Text("Pause pinning")
        case (.pins, true): Text("Resume pinning")
        case (.outputs, false): Text("Pause exports")
        case (.outputs, true): Text("Resume exports")
        }
    }

    /// Pause the whole queue, or resume it while paused, surfacing a
    /// failure in the Storage Manager window.
    private func setPaused() {
        let paused = !paused
        let queue = queue
        Task {
            do {
                switch queue {
                case .uploads: try await sync.setSyncPaused(paused)
                case .pins: try await downloads.setDownloadsPaused(paused)
                case .outputs: try await outputs.setOutputsPaused(paused)
                }
            }
            catch { uiStore.showError(error) }
        }
    }
}

/// How fast every file being sent goes together, read alone so a confirmed
/// part redraws only it. A zero reading shows nothing.
private struct StorageUploadRate: View {
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        if let throughputText = outboxStore.throughputText {
            Text(throughputText)
        }
    }
}

/// How fast the pin queue's running release has downloaded, read alone by
/// its release.
private struct StoragePinRate: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        if let rate = downloadStore.transfer(forRelease: releaseId)?.progress
            .throughputBps
        {
            Text(QueueSummary.throughputText(bytesPerSecond: rate))
        }
    }
}

/// How far the output queue's running release has been written, read alone
/// by its release.
private struct StorageOutputPercent: View {
    let releaseId: String
    @Environment(OutputStore.self)
    private var outputStore

    var body: some View {
        if let transfer = outputStore.transfer(forRelease: releaseId) {
            Text(Double(transfer.percent) / 100, format: .percent)
        }
    }
}

#if DEBUG
    /// The storage table's width the previews draw in.
    private let previewWidth: CGFloat = 700

    #Preview("Uploads, pinning and exports") {
        StorageFooter(
            list: PreviewData.storageList(),
            totalSize: 857_000_000,
            queues: BridgeStorageQueues(
                uploads: BridgeUploadQueueRun(paused: false),
                pins: BridgeReleaseQueueRun(paused: false, active: "rel-row-5"),
                outputs: BridgeReleaseQueueRun(paused: true, active: nil)
            )
        )
        .frame(width: previewWidth)
        .environment(PreviewData.outboxStore())
        .environment(PreviewData.downloadStore())
        .environment(PreviewData.outputStore())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(UiStore())
    }

    #Preview("Total not yet loaded") {
        StorageFooter(
            list: PreviewData.storageList(),
            totalSize: nil,
            queues: nil
        )
        .frame(width: previewWidth)
        .environment(PreviewData.outboxStore())
        .environment(PreviewData.downloadStore())
        .environment(PreviewData.outputStore())
        .environment(Sync.stub())
        .environment(Downloads.stub())
        .environment(Outputs.stub())
        .environment(UiStore())
    }
#endif
