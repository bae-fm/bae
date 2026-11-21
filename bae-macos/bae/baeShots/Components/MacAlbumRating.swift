import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumRating`, one shot for each row of its preview.
    static let macAlbumRatingScenes: [ShotScene] = [
        albumRating(
            "mac.component.album-rating.not-rated",
            BridgeAlbumRating(mine: nil, raters: [])
        ),
        albumRating(
            "mac.component.album-rating.rated",
            BridgeAlbumRating(mine: .four, raters: [])
        ),
        albumRating(
            "mac.component.album-rating.others-rated",
            ratedByYouAndAna
        ),
    ]

    private static func albumRating(
        _ id: String,
        _ rating: BridgeAlbumRating
    ) -> ShotScene {
        component(id, width: 284, height: 50) {
            MacAlbumRating(rating: rating, onRate: { _ in })
        }
    }
}
