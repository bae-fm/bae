import BaeKit
import SwiftUI

/// One download-queue row: album title, file count, size, a state chip, and a
/// cancel button; once the download failed, a button that queues every failed
/// download again. A pin and a release moving to This Mac read alike but for
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
                HStack(
                    alignment: .firstTextBaseline,
                    spacing: ThemeSpace.related
                ) {
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
            StatusChip("Queued to move to This Mac", symbol: "clock")
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
            Notice(
                title: op.kind == .pin
                    ? String(localized: "Could not pin")
                    : String(localized: "Could not move to This Mac"),
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
