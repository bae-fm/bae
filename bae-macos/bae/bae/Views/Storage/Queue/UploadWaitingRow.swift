import BaeKit
import SwiftUI

/// A release's upload that waits for a file whose original is missing, in
/// the Storage Manager's inspector: the release's title with its files and
/// size and the cancel mark; under them Waiting for a file, naming the file,
/// with Restore source file…, whose sheet says to put the file back and
/// retries the upload.
struct UploadWaitingRow: View {
    let title: String
    let fileCount: UInt64
    let totalSize: UInt64
    /// The name of the file whose original is missing.
    let missing: String
    let onCancel: () -> Void

    @State
    private var restoring = false

    var body: some View {
        QueueRow(
            icon: "arrow.up",
            createdAt: nil,
            cancel: .init(help: "Cancel", action: onCancel)
        ) {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Text(title)
                    .lineLimit(1)
                Text(
                    BridgeQueuedRelease.filesText(
                        fileCount: fileCount,
                        totalSize: totalSize
                    )
                )
                .themeText(.detail)
                .monospacedDigit()
                .foregroundStyle(.secondary)
            }
        } below: {
            Notice(
                title: String(localized: "Waiting for a file"),
                advice: absence,
                error: nil,
                tone: .warning,
                look: .quiet,
                action: ErrorAction("Restore source file…") {
                    restoring = true
                },
                buttons: .below
            )
        }
        .sheet(isPresented: $restoring) {
            RestoreFileSheet(
                heading: absence,
                detail: nil,
                onDismiss: { restoring = false }
            )
        }
    }

    private var absence: String {
        String(localized: "\(missing) is no longer where you imported it from.")
    }
}
