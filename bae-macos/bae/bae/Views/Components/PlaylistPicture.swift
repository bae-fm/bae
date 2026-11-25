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
            // The board's grid puts the split at the half rounded up, a
            // whole point: a 103 pt picture is 52 pt and 51 pt of covers.
            let near = (size / 2).rounded(.toNearestOrAwayFromZero)
            let far = size - near
            VStack(spacing: 0) {
                row(covers, from: 0, height: near, near: near, far: far)
                row(covers, from: 2, height: far, near: near, far: far)
            }
            .frame(width: size, height: size)
        }
    }

    private func row(
        _ covers: [BridgeImageRef?],
        from start: Int,
        height: CGFloat,
        near: CGFloat,
        far: CGFloat
    ) -> some View {
        HStack(spacing: 0) {
            ForEach(start..<start + 2, id: \.self) { index in
                let width = index == start ? near : far
                Artwork(
                    imageRef: index < covers.count ? covers[index] : nil,
                    pointSize: max(width, height),
                    cornerRadius: nil
                )
                .frame(width: width, height: height)
                .clipped()
            }
        }
    }
}
