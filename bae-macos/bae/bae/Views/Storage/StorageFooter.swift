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
                Text(verbatim: FactList.glyph)
                Text(
                    "Total: \(totalSize.fileSizeText)"
                )
            }
            Spacer()
            if let uploads = queues?.uploads {
                StorageQueueControl(
                    queue: .uploads,
                    paused: uploads.wait == .paused,
                    hold: uploads.wait?.text
                ) {
                    StorageUploadRate()
                }
            }
            if let downloads = queues?.downloads {
                StorageQueueControl(
                    queue: .downloads,
                    paused: downloads.wait == .paused,
                    hold: downloads.wait?.text
                ) {
                    if let active = downloads.active {
                        StoragePinRate(releaseId: active)
                    }
                }
            }
            if let outputs = queues?.outputs {
                StorageQueueControl(
                    queue: .outputs,
                    paused: outputs.wait == .paused,
                    hold: outputs.wait?.text
                ) {
                    if let active = outputs.active {
                        StorageOutputPercent(releaseId: active)
                    }
                }
            }
        }
        .lineLimit(1)
        .foregroundStyle(.secondary)
        .themeText(.body)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}

/// One running queue in the footer: its glyph, how fast it goes, or how far
/// for exports, which count percent rather than bytes, or in the rate's place
/// what holds the queue back, and the button that pauses it, or resumes it
/// while paused.
private struct StorageQueueControl<Progress: View>: View {
    let queue: StorageQueueKind
    let paused: Bool
    /// What holds the queue back, in words.
    let hold: String?
    @ViewBuilder
    let progress: () -> Progress

    var body: some View {
        HStack(spacing: ThemeSpace.compact) {
            HStack(spacing: ThemeSpace.inline) {
                glyph
                    .themeIcon(.badge)
                if let hold {
                    Text(hold)
                }
                else {
                    progress()
                        .monospacedDigit()
                }
            }
            QueuePauseButton(queue: queue, paused: paused)
                .buttonStyle(.borderless)
        }
        // A narrow table keeps each queue's rate whole on one line; the
        // release count and total give way instead.
        .fixedSize()
    }

    @ViewBuilder
    private var glyph: some View {
        switch queue {
        case .uploads:
            Image(systemName: "arrow.up")
                .foregroundStyle(StorageTransferKind.upload.color)
                .accessibilityLabel(Text("Uploads"))
        case .downloads:
            Image(systemName: "arrow.down")
                .foregroundStyle(StorageTransferKind.pin.color)
                .accessibilityLabel(Text("Downloads"))
        case .outputs:
            Image(systemName: "square.and.arrow.up")
                .accessibilityLabel(Text("Exports"))
        }
    }
}

/// How fast every file being sent goes together, which a native label reads
/// from the latest total each frame. A zero reading shows nothing.
private struct StorageUploadRate: View {
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        StreamLabel(stream: .uploadRate(outboxStore.totalProgress))
    }
}

/// How fast the pin queue's running release has downloaded, which a native
/// label reads from the release's latest progress each frame.
private struct StoragePinRate: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StreamLabel(
            stream: .pinRate(
                downloadStore.downloadProgress(forRelease: releaseId)
            )
        )
    }
}

/// How far the output queue's running release has been written, which a
/// native label reads from the release's latest percent each frame.
private struct StorageOutputPercent: View {
    let releaseId: String
    @Environment(OutputStore.self)
    private var outputStore

    var body: some View {
        StreamLabel(
            stream: .exportPercent(
                outputStore.exportProgress(forRelease: releaseId)
            )
        )
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
                uploads: BridgeUploadQueueRun(wait: nil),
                downloads: BridgeReleaseQueueRun(
                    wait: nil,
                    active: "rel-row-5"
                ),
                outputs: BridgeReleaseQueueRun(wait: .paused, active: nil)
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
