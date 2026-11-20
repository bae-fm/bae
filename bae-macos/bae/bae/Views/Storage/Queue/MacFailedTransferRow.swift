import BaeKit
import SwiftUI

/// A transfer that stopped, at the top of the Storage Manager's inspector:
/// the release's title, file count and size, and a cancel button, with a
/// notice under them. A pin and a release moving to This Mac read alike but
/// for their words; the notice of a failed download says why and offers
/// Retry, which queues every failed download again. An upload held by a
/// missing original names the file and offers Restore source file…, whose
/// sheet says to put the file back and retries the upload. A move to the
/// cloud that failed because a file's original changed names the file and
/// offers Details and Cancel upload, which ends the move.
struct MacFailedTransferRow: View {
    /// What the row shows.
    enum Entry {
        /// A pin or a move to This Mac that failed; core retries the
        /// queue's failures together, not this entry alone.
        case download(BridgeDownloadOp, onRetry: () -> Void)
        /// An upload that waits for the file `missing`, whose original is
        /// gone, with the release's album title and its files' count and
        /// size.
        case uploadWaiting(
            title: String,
            fileCount: UInt64,
            totalSize: UInt64,
            missing: String
        )
        /// A move to the cloud that failed because the original of the file
        /// `changed` is no longer what was imported, with the release's
        /// album title and its files' count and size, and the failure
        /// Details opens, whose detail is the text of the error behind it
        /// when coven kept one.
        case moveFailed(
            title: String,
            fileCount: UInt64,
            totalSize: UInt64,
            changed: String,
            reason: DisplayError
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
        case .download(let op, _): op.kind == .pin ? "pin" : "folder"
        case .uploadWaiting, .moveFailed: "arrow.up"
        }
    }

    private var title: String {
        switch entry {
        case .download(let op, _): op.titleText
        case .uploadWaiting(let title, _, _, _): title
        case .moveFailed(let title, _, _, _, _): title
        }
    }

    private var detail: String {
        switch entry {
        case .download(let op, _):
            op.detailText
        case .uploadWaiting(_, let fileCount, let totalSize, _),
            .moveFailed(_, let fileCount, let totalSize, _, _):
            BridgeQueuedRelease.filesText(
                fileCount: fileCount,
                totalSize: totalSize
            )
        }
    }

    /// The failed download's notice, the waiting upload's, or the failed
    /// move's, under the row.
    @ViewBuilder
    private var notice: some View {
        switch entry {
        case .download(let op, let onRetry):
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
        case .moveFailed(_, _, _, let changed, let reason):
            MacNotice(
                title: String(localized: "Could not move to the cloud"),
                advice: String(
                    localized: "\(changed) changed since import."
                ),
                error: reason,
                look: .quiet,
                action: ErrorAction("Cancel upload", perform: onCancel),
                buttons: .below
            )
        }
    }

    /// What the waiting upload's notice says of the file `missing`.
    private static func absence(of missing: String) -> String {
        String(localized: "\(missing) is no longer where you imported it from.")
    }
}
