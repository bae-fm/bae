import BaeKit
import SwiftUI

/// One album card in an artist's detail-pane grid: cover over title and year.
struct MacArtistAlbumTile: View {
    let album: BridgeAlbum
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                Artwork(
                    imageRef: album.cover,
                    pointSize: 140,
                    cornerRadius: nil
                )
                .aspectRatio(1, contentMode: .fit)
                .clipShape(
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                )
                Text(album.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                StableOptionalText(
                    text: album.year.map(String.init),
                    role: .detail,
                    foreground: .secondary,
                    lineLimit: 1
                )
            }
        }
        .buttonStyle(.accentPlain)
    }
}
