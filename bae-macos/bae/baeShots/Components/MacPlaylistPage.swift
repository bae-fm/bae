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
                sharing: .household,
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
        component(id, width: 404, height: 240, images: coverStore()) {
            let backing = boardLibraryBacking(uiStore: UiStore())
            MacPlaylistPage(
                detail: BridgePlaylistDetail(
                    summary: summary,
                    entries: [],
                    empty: true
                )
            )
            .albumDetailPreviewEnvironment()
            .environment(backing.session)
            .frame(width: 404, alignment: .leading)
            .overlay(alignment: .topTrailing) {
                if optionsOpen {
                    // Under the ⋯ at the page's right: the page's margin and
                    // the picture's height down, the button's height further.
                    MacMenuInPlace {
                        PlaylistOptionsMenu(
                            onShuffle: {},
                            owner: PlaylistOptionsMenu.OwnerActions(
                                edit: {},
                                delete: {}
                            )
                        )
                    }
                    .padding(.top, ThemeSpace.section + 50 + 4)
                    .padding(.trailing, ThemeSpace.section)
                }
            }
        }
    }
}
