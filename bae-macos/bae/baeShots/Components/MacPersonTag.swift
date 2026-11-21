import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacPersonTag` as a history row ends in it, and as a playlist header's
    /// owner line.
    static let macPersonTagScenes: [ShotScene] = [
        personTag("mac.component.person-tag.resting"),
        personTag("mac.component.person-tag.strong", strong: true),
    ]

    private static func personTag(_ id: String, strong: Bool = false)
        -> ShotScene
    {
        component(id, height: 40) {
            MacPersonTag(
                person: BridgePersonRef(
                    person: "person-ana",
                    name: "Ana",
                    initials: "A",
                    avatarVersion: nil
                ),
                strong: strong
            )
        }
    }
}
