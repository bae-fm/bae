import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacActivityButton` as the title bar shows it while bytes move, while
    /// a problem is listed and while a queue is held.
    static let macActivityButtonScenes: [ShotScene] = [
        activityButton(
            "mac.component.activity-button.moving",
            movingActivity
        ),
        activityButton(
            "mac.component.activity-button.attention",
            attentionActivity
        ),
        activityButton(
            "mac.component.activity-button.paused",
            pausedActivity
        ),
    ]

    private static func activityButton(
        _ id: String,
        _ activity: BridgeSyncActivity
    ) -> ShotScene {
        component(id, width: 64, height: 60) {
            MacActivityButton()
                .environment(activityStore(activity))
                .environment(SubsonicStore())
                .environment(SettingsNavigation())
        }
    }
}
