import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacTrackLead`: the number, Play under the pointer, the speaker on the
    /// current track and Pause under the pointer on it.
    static let macTrackLeadScenes: [ShotScene] = [
        trackLead("mac.component.track-lead.number"),
        trackLead("mac.component.track-lead.hover", hover: true),
        trackLead("mac.component.track-lead.current", current: true),
        trackLead(
            "mac.component.track-lead.pause",
            current: true,
            hover: true
        ),
    ]

    private static func trackLead(
        _ id: String,
        current: Bool = false,
        hover: Bool = false
    ) -> ShotScene {
        component(id, width: 84, height: 50) {
            MacTrackLead(
                text: "4",
                isCurrent: current,
                isPlaying: current,
                hover: hover,
                width: MacTrackRow.leadingSlotMinWidth,
                onPlay: {},
                onTogglePlayPause: {}
            )
        }
    }
}
