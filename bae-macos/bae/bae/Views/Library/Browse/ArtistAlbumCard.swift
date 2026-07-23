import BaeKit
import SwiftUI

/// One album card in an artist's detail-pane grid: cover over title and year.
struct ArtistAlbumCard: View {
    let album: BridgeAlbum
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 6) {
                ImageView(imageRef: album.cover, pointSize: 140)
                    .aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(album.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                StableOptionalText(
                    text: album.year.map(String.init),
                    font: .caption,
                    foreground: .secondary,
                    lineHeight: 12,
                    lineLimit: 1
                )
            }
        }
        .buttonStyle(.plain)
    }
}
