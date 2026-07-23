import BaeKit
import SwiftUI

/// File row content (a release's expanded child): filename in the outline
/// column, audio-format descriptor in Format, formatted size in Size. The
/// artist / storage / files columns stay blank for files.
struct StorageFileCell: View {
    let file: BridgeFile
    let column: StorageTableColumn

    var body: some View {
        Group {
            switch column {
            case .album:
                Text(file.originalFilename)
                    .lineLimit(1)
                    .foregroundStyle(.secondary)
            case .format:
                // Audio files carry a label; non-audio files (images, cue)
                // have none, so their Format cell is simply empty.
                if let format = file.audioFormat {
                    Text(format.text)
                        .lineLimit(1)
                        .foregroundStyle(.secondary)
                }
            case .size:
                Text(file.fileSizeText)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .artist, .storage, .files:
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: cellAlignment(column))
        .padding(.horizontal, 4)
    }
}
