import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Artwork`: a cover, the tile of a release with none, and a round
    /// picture.
    static let artworkScenes: [ShotScene] = [
        component(
            "mac.component.artwork.cover",
            height: 136,
            images: coverStore()
        ) {
            Artwork(imageRef: cover("Low Tide Hours"), pointSize: 120)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
        },
        component(
            "mac.component.artwork.missing",
            height: 136,
            images: coverStore()
        ) {
            Artwork(imageRef: nil, pointSize: 120)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
        },
        component(
            "mac.component.artwork.round",
            height: 64,
            images: coverStore()
        ) {
            Artwork(imageRef: cover("Mara Quill"), pointSize: 48)
                .frame(width: 48, height: 48)
                .clipShape(Circle())
        },
    ]
}
