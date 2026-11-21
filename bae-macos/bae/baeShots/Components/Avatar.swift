import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `Avatar`: a person's initials, and their picture.
    static let avatarScenes: [ShotScene] = [
        component("mac.component.avatar.initials", height: 48) {
            Avatar(initials: "MQ", size: 32)
        },
        component(
            "mac.component.avatar.picture",
            height: 48,
            images: ImageStore(fetchAvatarBytes: {
                PreviewData.previewArtData($0)
            })
        ) {
            Avatar(
                initials: "MQ",
                picture: (person: "Mara", version: "1"),
                size: 32
            )
        },
    ]
}
