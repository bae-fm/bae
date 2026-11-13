import BaeKit
import SwiftUI

/// A release row's content for one column; needs `DownloadStore`,
/// `OutboxStore` and `ImageStore` in the environment.
struct StorageReleaseCell: View {
    let release: ReleaseSummary
    let album: BridgeAlbum
    let status: BridgeStorageRowStatus
    let dateAdded: Date
    let column: StorageTableColumn

    /// The release column's cover side.
    private static let coverSize: CGFloat = 24

    var body: some View {
        Group {
            switch column {
            case .release:
                HStack(spacing: ThemeSpace.related) {
                    ImageView(
                        imageRef: release.cover,
                        pointSize: Self.coverSize
                    )
                    .frame(width: Self.coverSize, height: Self.coverSize)
                    .clipShape(
                        RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                    )
                    Text(album.title).lineLimit(1)
                }
            case .artist:
                Text(album.artistLine.text).lineLimit(1)
            case .media:
                Text(release.media.isEmpty ? "\u{2014}" : release.mediaText)
            case .storage:
                MacStorageChip(release: release, status: status)
            case .files:
                Text(verbatim: release.fileCount.formatted())
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .size:
                Text(release.totalSizeText)
                    .monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .trailing)
            case .dateAdded:
                Text(dateAdded, format: .dateTime.year().month().day())
                    .monospacedDigit()
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: cellAlignment(column))
        .padding(.horizontal, ThemeSpace.inline)
    }
}
