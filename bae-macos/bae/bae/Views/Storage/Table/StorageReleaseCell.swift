import BaeKit
import SwiftUI

/// Release row content for one column. Reads the storage badge's `OutboxStore`
/// and the cover's `MediaPaths` at the leaf (injected on the hosted view).
struct StorageReleaseCell: View {
    let release: ReleaseSummary
    let album: AlbumSummary
    let column: StorageTableColumn

    var body: some View {
        Group {
            switch column {
            case .album:
                HStack(spacing: 8) {
                    ImageView(imageRef: release.cover, pointSize: 24)
                        .frame(width: 24, height: 24)
                        .clipShape(RoundedRectangle(cornerRadius: 3))
                    Text(album.title).lineLimit(1)
                }
            case .artist:
                Text(album.artistNames).lineLimit(1)
            case .format:
                Text(release.format ?? "\u{2014}")
            case .storage:
                StorageStateLabel(release: release)
            case .files:
                Text(verbatim: release.fileCount.formatted())
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .size:
                Text(release.totalSizeText)
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
        .frame(maxWidth: .infinity, alignment: cellAlignment(column))
        .padding(.horizontal, 4)
    }
}
