import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPlaylistHeader`: the person's own playlist shared with everyone
    /// and private, another person's, and the options menu open.
    static let macPlaylistHeaderScenes: [ShotScene] = [
        playlistHeader(
            "mac.component.playlist-header.only-me",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .private,
                yours: true
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.everyone",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.no-options",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: false
            )
        ),
        playlistHeader(
            "mac.component.playlist-header.options-open",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            ),
            height: 230,
            optionsOpen: true
        ),
    ]

    private static func playlistHeader(
        _ id: String,
        summary: BridgePlaylistSummary,
        height: CGFloat = 110,
        optionsOpen: Bool = false
    ) -> ShotScene {
        component(id, width: 404, height: height, images: coverStore()) {
            MacPlaylistHeader(summary: summary, onEdit: {}, onDelete: {})
                .environment(Playback.stub())
                .frame(width: 404, alignment: .leading)
                .overlay(alignment: .topTrailing) {
                    if optionsOpen {
                        // Under the ⋯ at the header's right, which is centred
                        // beside the 72pt picture.
                        MacMenuInPlace {
                            PlaylistOptionsMenu(
                                onShuffle: {},
                                owner: PlaylistOptionsMenu.OwnerActions(
                                    edit: {},
                                    delete: {}
                                )
                            )
                        }
                        .padding(.top, 54)
                    }
                }
        }
    }
}
