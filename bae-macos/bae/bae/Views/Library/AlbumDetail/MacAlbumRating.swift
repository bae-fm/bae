import BaeKit
import SwiftUI

/// The album's stars and who rated it. The person clicks the stars to rate the
/// album.
struct MacAlbumRating: View {
    /// The stars the person gave the album.
    let mine: BridgeStars?
    /// Who rated the album, this person first.
    let raters: [BridgeRater]
    /// Rate the album with these stars, or clear its rating with none.
    let onRate: (BridgeStars?) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            Stars(
                value: mine?.number ?? 0,
                size: 16,
                onRate: { onRate($0.afterTapping(from: mine)) }
            )
            Text(verbatim: raters.text)
                .themeText(.detail)
                .foregroundStyle(Theme.macTextSecondary)
        }
        .accessibilityIdentifier("album-rating")
    }
}
