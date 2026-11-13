import BaeKit
import SwiftUI

/// A playlist's picture: one cover filling the square, or four covers as a
/// 2×2 square. A missing cover draws the missing-art tile in its place.
struct PlaylistPicture: View {
    let cover: BridgePlaylistCover
    let size: CGFloat

    /// The gap between the four covers of a grid.
    private static let gap: CGFloat = 1

    var body: some View {
        switch cover {
        case .single(let cover):
            ImageView(imageRef: cover, pointSize: size)
                .frame(width: size, height: size)
        case .grid(let covers):
            let cell = (size - Self.gap) / 2
            VStack(spacing: Self.gap) {
                row(covers, from: 0, cell: cell)
                row(covers, from: 2, cell: cell)
            }
            .frame(width: size, height: size)
        }
    }

    private func row(
        _ covers: [BridgeImageRef?],
        from start: Int,
        cell: CGFloat
    ) -> some View {
        HStack(spacing: Self.gap) {
            ForEach(start..<start + 2, id: \.self) { index in
                ImageView(
                    imageRef: index < covers.count ? covers[index] : nil,
                    pointSize: cell
                )
                .frame(width: cell, height: cell)
                .clipped()
            }
        }
    }
}
