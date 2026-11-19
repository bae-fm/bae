import BaeKit
import SwiftUI

/// The Release column's cell: a cover and the album's title.
struct MacReleaseCell: View {
    let release: ReleaseSummary
    let album: BridgeAlbum

    private static let coverSize: CGFloat = 24

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Artwork(
                imageRef: release.cover,
                pointSize: Self.coverSize
            )
            .frame(width: Self.coverSize, height: Self.coverSize)
            .clipShape(
                RoundedRectangle(cornerRadius: ThemeRadius.artwork)
            )
            Text(album.title).lineLimit(1)
        }
    }
}
