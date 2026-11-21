import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `PlaylistPicture`: four releases' covers, fewer than four, and none.
    static let playlistPictureScenes: [ShotScene] = [
        component(
            "mac.component.playlist-picture.four-releases",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(
                .grid(
                    covers: [
                        cover("Low Tide Hours"), cover("Copper Sky"),
                        cover("Small Hours"), cover("Paper Lanterns"),
                    ]
                )
            )
        },
        component(
            "mac.component.playlist-picture.fewer-than-four",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(.single(cover: cover("Copper Sky")))
        },
        component(
            "mac.component.playlist-picture.empty",
            height: 136,
            images: coverStore()
        ) {
            playlistPicture(.single(cover: nil))
        },
    ]

    private static func playlistPicture(_ cover: BridgePlaylistCover)
        -> some View
    {
        PlaylistPicture(cover: cover, size: 120)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.cover))
    }
}
