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
            width: wideColumn,
            height: 70,
            images: coverGalleryImages()
        ) {
            MacReleaseCard(group: group)
                .frame(width: wideColumn, alignment: .leading)
        }
    }
}
