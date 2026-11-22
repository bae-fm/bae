import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacImportImages`, one shot for each row of its preview.
    static let macImportImagesScenes: [ShotScene] = [
        importImagesScene(
            "mac.component.import-images.several",
            names: ["cover", "back", "disc"]
        ),
        importImagesScene("mac.component.import-images.one", names: ["cover"]),
    ]

    /// `MacImportImages` over `names`, the first the cover and the others
    /// its back and disc, each drawn as the release's artwork.
    private static func importImagesScene(
        _ id: String,
        names: [String]
    ) -> ShotScene {
        let art = ["cover": "", "back": " back", "disc": " disc"]
        return component(
            id,
            width: 580,
            height: 190,
            images: PreviewData.artImageStore()
        ) {
            MacImportImages(
                images: try names.map {
                    try PreviewData.boardImage(
                        "\($0).jpg",
                        art: "Low Tide Hours\(art[$0] ?? "")"
                    )
                },
                onOpen: { _ in }
            )
            .frame(width: 580, alignment: .leading)
        }
    }
}
