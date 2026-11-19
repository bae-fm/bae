import BaeKit
import SwiftUI

/// A release's row in the Storage Manager's table, drawn one column at a
/// time: the cover and title, the artist, the media, the storage, and the
/// numbers. Needs `DownloadStore`, `OutboxStore` and `ImageStore` in the
/// environment.
struct MacStorageRow: View {
    let release: ReleaseSummary
    let album: BridgeAlbum
    let status: BridgeStorageRowStatus
    let dateAdded: Date
    let column: StorageTableColumn

    var body: some View {
        Group {
            switch column {
            case .release:
                MacReleaseCell(release: release, album: album)
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
