import BaeKit
import SwiftUI

/// A playlist's picture: one cover filling the square, or four covers as a
/// 2×2 square. A missing cover draws the missing-art tile in its place.
struct PlaylistPicture: View {
    let cover: BridgePlaylistCover
    let size: CGFloat

    var body: some View {
        switch cover {
        case .single(let cover):
            Artwork(imageRef: cover, pointSize: size, cornerRadius: nil)
                .frame(width: size, height: size)
        case .grid(let covers):
            // The four covers touch, as the board and Android draw them.
            let cell = size / 2
            VStack(spacing: 0) {
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
        HStack(spacing: 0) {
            ForEach(start..<start + 2, id: \.self) { index in
                Artwork(
                    imageRef: index < covers.count ? covers[index] : nil,
                    pointSize: cell,
                    cornerRadius: nil
                )
                .frame(width: cell, height: cell)
                .clipped()
            }
        }
    }
}
