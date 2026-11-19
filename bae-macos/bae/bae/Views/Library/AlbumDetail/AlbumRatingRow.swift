import BaeKit
import SwiftUI

/// The album's stars and who rated it. The person clicks the stars to rate the
/// album.
struct AlbumRatingRow: View {
    let rating: BridgeAlbumRating
    /// Rate the album with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Stars(
                value: rating.mine?.number ?? 0,
                size: 16,
                onRate: { onRate(rating.stars(afterTapping: $0)) }
            )
            Text(verbatim: rating.ratersText)
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
        .accessibilityIdentifier("album-rating")
    }
}
