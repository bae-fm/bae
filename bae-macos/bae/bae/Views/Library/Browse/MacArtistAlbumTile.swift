import BaeKit
import SwiftUI

/// One album card in an artist's detail-pane grid: cover over title and year.
/// When only another device holds the album's files, the cover and words
/// show dimmed, and the card still opens the album.
struct MacArtistAlbumTile: View {
    let entry: BridgeArtistAlbum
    let onTap: () -> Void

    private var album: BridgeAlbum { entry.album }
    private var textOpacity: Double {
        entry.primaryReleaseUnplayable ? ThemeOpacity.dimmed : 1
    }

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                Artwork(
                    imageRef: album.cover,
                    pointSize: 140,
                    cornerRadius: nil,
                    dimmed: entry.primaryReleaseUnplayable
                )
                .aspectRatio(1, contentMode: .fit)
                .clipShape(
                    RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                )
                Text(album.title)
                    .themeText(.rowTitle)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .opacity(textOpacity)
                    .accessibilityHint(
                        "Not on this device",
                        isEnabled: entry.primaryReleaseUnplayable
                    )
                StableOptionalText(
                    text: album.year.map(String.init),
                    role: .detail,
                    foreground: .secondary,
                    lineLimit: 1
                )
                .opacity(textOpacity)
            }
        }
        .buttonStyle(.accentPlain)
    }
}
