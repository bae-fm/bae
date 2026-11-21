import BaeKit
import SwiftUI

/// A release's files in its Storage sheet: a sortable table of each file's
/// name, audio format, size and kind. The rows come in the sort's order from
/// core, so the table sorts nothing itself: each column's comparator only
/// names what it sorts by.
struct MacReleaseFileTable: View {
    let files: [BridgeFile]
    @Binding
    var sortOrder: [KeyPathComparator<BridgeFile>]

    var body: some View {
        Table(files, sortOrder: $sortOrder) {
            TableColumn(
                "Name",
                value: \.originalFilename,
                comparator: .localizedStandard
            ) { file in
                Text(file.originalFilename).cutLine(.fileName)
            }
            TableColumn(localizedCoreString("core.audio.label")) { file in
                // Non-audio files (images, cue) leave the cell empty.
                if let format = file.audioFormat {
                    Text(format.text)
                        .foregroundStyle(Theme.macTextSecondary)
                        .lineLimit(1)
                }
            }
            TableColumn("Size", value: \.fileSize) { file in
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(Theme.macTextSecondary)
            }
            .width(min: 70, ideal: 80)
            TableColumn("Kind", value: \.contentType) { file in
                Text(file.contentType)
                    .foregroundStyle(Theme.textTertiary)
                    .lineLimit(1)
            }
            .width(min: 90, ideal: 130)
        }
    }
}
