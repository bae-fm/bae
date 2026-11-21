import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacArtistOtherNames`, one shot for each row of its preview.
    static let macArtistOtherNamesScenes: [ShotScene] = [
        component("mac.component.artist-other-names.empty", height: 90) {
            MacArtistOtherNames(artistId: "artist-mara-quill", names: [])
                .frame(width: 480, alignment: .leading)
                .environment(ArtistEditor())
                .environment(UiStore())
        },
        component("mac.component.artist-other-names.typed", height: 120) {
            MacArtistOtherNames(
                artistId: "artist-mara-quill",
                names: [
                    BridgeArtistCreditName(
                        artistId: "artist-mara-quill",
                        credit: BridgeCreditName(key: "mq", name: "M. Quill")
                    )
                ]
            )
            .frame(width: 480, alignment: .leading)
            .environment(ArtistEditor())
            .environment(UiStore())
        },
    ]
}
