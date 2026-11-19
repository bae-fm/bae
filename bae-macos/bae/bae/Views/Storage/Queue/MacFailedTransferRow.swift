import BaeKit
import SwiftUI

/// One transfer row in the Storage Manager's inspector: the release's title,
/// file count and size, a state chip, and a cancel button. A pin and a
/// release moving to This Mac read alike but for their words and the
/// progress they follow; once the download failed, a notice says why and
/// offers Retry, which queues every failed download again. An upload held by
/// a missing original names the file and offers Restore source file…, whose
/// sheet says to put the file back and retries the upload.
struct MacFailedTransferRow: View {
    /// What the row shows.
    enum Entry {
        /// A pin or a move to This Mac. `transfer` is the latest progress of
        /// the download, which the row's native readout reads each display
        /// frame while it runs; core retries the queue's failures together,
        /// not this entry alone.
        case download(
            BridgeDownloadOp,
            transfer: ReleaseProgressSource,
            onRetry: () -> Void
        )
        /// An upload that waits for the file `missing`, whose original is
        /// gone, with the release's album title and its files' count and
        /// size.
        case uploadWaiting(
            title: String,
            fileCount: UInt64,
            totalSize: UInt64,
            missing: String
        )
    }

    let entry: Entry
    let onCancel: () -> Void

    @State
    private var restoring = false

    var body: some View {
        QueueRow(
            icon: icon,
            createdAt: nil,
            cancel: .init(help: "Cancel", action: onCancel)
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                HStack(
                    alignment: .firstTextBaseline,
                    spacing: ThemeSpace.related
                ) {
                    Text(title)
                        .lineLimit(1)

                    Text(detail)
                        .themeText(.detail)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                progressView
                stateChip
            }
        } below: {
            notice
        }
        .sheet(isPresented: $restoring) {
            if case .uploadWaiting(_, _, _, let missing) = entry {
                RestoreFileSheet(
                    heading: Self.absence(of: missing),
                    detail: nil,
                    onDismiss: { restoring = false }
                )
            }
        }
    }

    private var icon: String {
        switch entry {
        case .download(let op, _, _): op.kind == .pin ? "pin" : "folder"
        case .uploadWaiting: "arrow.up"
        }
    }

    private var title: String {
        switch entry {
        case .download(let op, _, _): op.titleText
        case .uploadWaiting(let title, _, _, _): title
        }
    }

    private var detail: String {
        switch entry {
        case .download(let op, _, _):
            op.detailText
        case .uploadWaiting(_, let fileCount, let totalSize, _):
            BridgeQueuedRelease.filesText(
                fileCount: fileCount,
                totalSize: totalSize
            )
        }
    }

    /// The queued or running state of a download, under the title line.
    @ViewBuilder
    private var stateChip: some View {
        if case .download(let op, _, _) = entry {
            switch (op.kind, op.state) {
            case (.pin, .queued):
                StatusChip("Queued to pin", symbol: "clock")
            case (.moveToThisDevice, .queued):
                StatusChip("Queued to move to This Mac", symbol: "clock")
            case (.pin, .active):
                StatusChip(
                    "Pinning",
                    tone: .activity,
                    symbol: "arrow.down.circle.fill"
                )
            case (.moveToThisDevice, .active):
                StatusChip(
                    verbatim: BridgeReleaseStorageAction.moveToThisDevice
                        .transferLabel,
                    tone: .activity,
                    symbol: "arrow.down.circle.fill"
                )
            case (_, .failed):
                EmptyView()
            }
        }
    }

    /// The failed download's notice, or the waiting upload's, under the row.
    @ViewBuilder
    private var notice: some View {
        switch entry {
        case .download(let op, _, let onRetry):
            if case .failed(let error) = op.state {
                MacNotice(
                    title: op.kind == .pin
                        ? String(localized: "Could not pin")
                        : String(localized: "Could not move to This Mac"),
                    advice: nil,
                    error: DisplayError(line: error),
                    look: .quiet,
                    action: ErrorAction(perform: onRetry)
                )
            }
        case .uploadWaiting(_, _, _, let missing):
            MacNotice(
                title: String(localized: "Waiting for a file"),
                advice: Self.absence(of: missing),
                error: nil,
                tone: .warning,
                look: .quiet,
                action: ErrorAction("Restore source file…") {
                    restoring = true
                },
                buttons: .below
            )
        }
    }

    @ViewBuilder
    private var progressView: some View {
        if case .download(let op, let transfer, _) = entry,
            case .active = op.state
        {
            MacTransferReadout(stream: transfer, kind: .pin)
        }
    }

    /// What the waiting upload's notice says of the file `missing`.
    private static func absence(of missing: String) -> String {
        String(localized: "\(missing) is no longer where you imported it from.")
    }
}
