import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacReleaseCard`: one title, and two titles in one line.
    static let macReleaseCardScenes: [ShotScene] = [
        releaseCardScene(
            "mac.component.release-card.default",
            lowTideHoursGroup()
        ),
        releaseCardScene(
            "mac.component.release-card.long-title",
            lowTideHoursGroup(
                titles: ["Low Tide Hours", "Low Tide Hours (Remastered)"],
                sources: [.musicBrainz]
            )
        ),
    ]

    private static func releaseCardScene(
        _ id: String,
        _ group: ReleaseGroup
    ) -> ShotScene {
        component(
            id,
            width: 842,
            height: 60,
            images: coverGalleryImages()
        ) {
            MacPressingTable(kinds: []) {
                MacReleaseCard(group: group)
            }
            .frame(width: 900, alignment: .leading)
            // The preview's table is 900 points wide in a 842 point cell, which
            // cuts it at the cell's edge.
            .frame(width: 842, alignment: .leading)
            .clipped()
        }
    }
}
