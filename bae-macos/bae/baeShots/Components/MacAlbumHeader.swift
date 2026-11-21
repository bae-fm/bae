import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumHeader` at rest, over two releases, on another Mac's disk,
    /// rated, with its menu open and with no year.
    static let macAlbumHeaderScenes: [ShotScene] = [
        albumHeader("mac.component.album-header.resting", panelDetail()),
        albumHeader(
            "mac.component.album-header.two-releases",
            panelDetail(releases: ["CD", "LP"])
        ),
        albumHeader(
            "mac.component.album-header.unplayable",
            panelDetail(playability: onAnasMac)
        ),
        albumHeader(
            "mac.component.album-header.rated",
            panelDetail(rating: ratedByYouAndAna)
        ),
        albumHeader(
            "mac.component.album-header.options-open",
            panelDetail(),
            height: 340,
            optionsOpen: true
        ),
        albumHeader(
            "mac.component.album-header.no-year",
            panelDetail(year: nil)
        ),
    ]

    private static func albumHeader(
        _ id: String,
        _ detail: AlbumDetail,
        height: CGFloat = 250,
        optionsOpen: Bool = false
    ) -> ShotScene {
        component(id, width: 540, height: height) {
            MacAlbumHeader(
                detail: detail,
                currentReleaseId: nil,
                onSelectRelease: { _ in },
                onPlay: {},
                onRate: { _ in }
            ) {
                MacMenuItem("Edit…") {}
                MacMenuItem("Change cover…") {}
                MacMenuItem("Storage…") {}
                MacMenuItem.divider
                MacMenuItem("Delete album…") {}
            }
            .frame(width: 540, alignment: .leading)
            .overlay(alignment: .bottomLeading) {
                if optionsOpen {
                    // Under the ⋯, which follows Play above the rating line.
                    MacMenuInPlace(
                        {
                            MacMenuItem("Edit…") {}
                            MacMenuItem("Change cover…") {}
                            MacMenuItem("Storage…") {}
                            MacMenuItem.divider
                            MacMenuItem("Delete album…") {}
                        },
                        highlighted: 0
                    )
                    .alignmentGuide(.bottom) { $0[.top] + 22 }
                    .padding(.leading, 64)
                }
            }
        }
    }
}
