import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPlaylistPage` for the person's own playlist shared with everyone
    /// and kept private, with its menu open, and for one another person
    /// shared.
    static let macPlaylistPageScenes: [ShotScene] = [
        playlistPage(
            "mac.component.playlist-page.only-me",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .private,
                yours: true
            )
        ),
        playlistPage(
            "mac.component.playlist-page.everyone",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: true
            )
        ),
        playlistPage(
            "mac.component.playlist-page.no-options",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .household,
                yours: false
            )
        ),
        playlistPage(
            "mac.component.playlist-page.options-open",
            summary: playlistSummary(
                owner: "Ana",
                sharing: .private,
                yours: true
            ),
            optionsOpen: true
        ),
    ]

    private static func playlistPage(
        _ id: String,
        summary: BridgePlaylistSummary,
        optionsOpen: Bool = false
    ) -> ShotScene {
        // The store the capture waits on is the one the page draws from.
        let images = coverStore()
        return component(id, width: 404, height: 240, images: images) {
            let backing = boardLibraryBacking(uiStore: UiStore())
            MacPlaylistPage(
                detail: BridgePlaylistDetail(
                    summary: summary,
                    entries: [],
                    empty: true
                )
            )
            .albumDetailPreviewEnvironment()
            // After the preview environment, which gives an empty image store.
            .environment(images)
            .environment(backing.session)
            .frame(width: 404, alignment: .leading)
            .overlay(alignment: .topLeading) {
                if optionsOpen {
                    // Under the ⋯, which follows Play: the menu's right edge
                    // is the button's, 234.5 points in, and its top 4 points
                    // under the button's bottom.
                    MacMenuInPlace(
                        {
                            PlaylistOptionsMenu(
                                onShuffle: {},
                                owner: PlaylistOptionsMenu.OwnerActions(
                                    edit: {},
                                    delete: {}
                                )
                            )
                        },
                        highlighted: 0
                    )
                    .padding(.leading, 234.5 - 141)
                    .padding(.top, 119)
                }
            }
        }
    }
}
