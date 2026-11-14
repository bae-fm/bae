import BaeKit
import SwiftUI

/// The album's stars and who rated it. The stars rate only for a person who
/// can; a person who can't still reads who did.
struct AlbumRatingRow: View {
    let rating: BridgeAlbumRating
    /// Rate the album with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            if rating.canRate {
                Stars(
                    value: rating.mine?.number ?? 0,
                    size: 16,
                    onRate: { onRate(rating.stars(afterTapping: $0)) }
                )
            }
            Text(verbatim: rating.ratersText)
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
        .accessibilityIdentifier("album-rating")
    }
}
