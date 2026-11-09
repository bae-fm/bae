import BaeKit
import SwiftUI

/// One download-queue row: album title, file count, size, a state chip, and a
/// cancel button; once the download failed, a button that queues every failed
/// download again. A pin and a release being made local read alike but for
/// their words and the progress they follow.
struct DownloadRow: View {
    let op: BridgeDownloadOp
    /// The latest progress of the download, which the row's native readout
    /// reads each display frame while the download runs.
    let transfer: ReleaseProgressSource
    /// Core retries the queue's failures together, not this entry alone.
    let onRetry: () -> Void
    let onCancel: () -> Void

    var body: some View {
        QueueRow(
            icon: op.kind == .pin ? "pin" : "folder",
            createdAt: nil,
            cancel: .init(help: "Cancel", action: onCancel)
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                HStack(spacing: ThemeSpace.related) {
                    Text(op.titleText)
                        .lineLimit(1)

                    if let release = op.release {
                        Text(release.detailText)
                            .themeText(.detail)
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                    }
                }
                progressView
                stateChip
            }
        } below: {
            failure
        }
    }

    /// The queued or running state, under the title line.
    @ViewBuilder
    private var stateChip: some View {
        switch (op.kind, op.state) {
        case (.pin, .queued):
            StatusChip("Queued to pin", symbol: "clock")
        case (.makeLocal, .queued):
            StatusChip("Queued to make local", symbol: "clock")
        case (.pin, .active):
            StatusChip(
                "Pinning",
                tone: .activity,
                symbol: "arrow.down.circle.fill"
            )
        case (.makeLocal, .active):
            StatusChip(
                verbatim: BridgeReleaseStorageAction.detachAudio.transferLabel,
                tone: .activity,
                symbol: "arrow.down.circle.fill"
            )
        case (_, .failed):
            EmptyView()
        }
    }

    /// The failed download's notice, under the row.
    @ViewBuilder
    private var failure: some View {
        if case .failed(let error) = op.state {
            ErrorNotice(
                title: op.kind == .pin
                    ? String(localized: "Couldn't pin")
                    : String(localized: "Couldn't make local"),
                advice: nil,
                error: DisplayError(line: error),
                look: .quiet,
                action: ErrorAction(perform: onRetry)
            )
        }
    }

    @ViewBuilder
    private var progressView: some View {
        if case .active = op.state {
            TransferReadout(stream: transfer, kind: .pin)
        }
    }
}

#if DEBUG
    #Preview("Download states") {
        VStack(spacing: 0) {
            ForEach(PreviewData.downloadOps, id: \.releaseId) { op in
                DownloadRow(
                    op: op,
                    transfer: .pin(
                        PreviewData.downloadStore()
                            .downloadProgress(forRelease: op.releaseId)
                    ),
                    onRetry: {},
                    onCancel: {}
                )
                Divider()
            }
        }
        .frame(width: 640)
        .padding(.vertical)
    }
#endif
