import BaeKit
import SwiftUI

/// The artist's picture, name and album count over their albums in the
/// grouped album grid. The picture is the artist list's: `ThemeSize.rowArtwork`
/// wide with the artwork corner, and the missing artwork when the artist has
/// no picture.
struct MacArtistGroupHeading: View {
    let name: String
    let picture: BridgeImageRef?
    let countLabel: BridgeCountLabel

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Artwork(
                imageRef: picture,
                pointSize: ThemeSize.rowArtwork,
                cornerRadius: nil
            )
            .frame(
                width: ThemeSize.rowArtwork,
                height: ThemeSize.rowArtwork
            )
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(verbatim: name)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(verbatim: countLabel.localizedLine)
                    .themeText(.detail)
                    .foregroundStyle(Theme.textSecondary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, ThemeSpace.compact)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}
