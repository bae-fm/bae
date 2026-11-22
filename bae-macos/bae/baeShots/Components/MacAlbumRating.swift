import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumRating`, one shot for each row of its preview.
    static let macAlbumRatingScenes: [ShotScene] = [
        albumRating(
            "mac.component.album-rating.not-rated",
            mine: nil,
            raters: []
        ),
        albumRating(
            "mac.component.album-rating.rated",
            mine: .four,
            raters: []
        ),
        albumRating(
            "mac.component.album-rating.others-rated",
            mine: .four,
            raters: youAndAnaRaters
        ),
    ]

    private static func albumRating(
        _ id: String,
        mine: BridgeStars?,
        raters: [BridgeRater]
    ) -> ShotScene {
        component(id, width: 284, height: 50) {
            MacAlbumRating(mine: mine, raters: raters, onRate: { _ in })
        }
    }
}
