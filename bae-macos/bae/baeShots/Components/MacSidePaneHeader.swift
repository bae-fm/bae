import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSidePaneHeader`, one shot for each row of its preview.
    static let macSidePaneHeaderScenes: [ShotScene] = [
        sidePaneHeader("mac.component.side-pane-header.queue", .queue),
        sidePaneHeader(
            "mac.component.side-pane-header.playlists",
            .playlists,
            lead: .newPlaylist {}
        ),
        sidePaneHeader(
            "mac.component.side-pane-header.playlist-open",
            .playlists,
            lead: .back(to: "Playlists") {}
        ),
        sidePaneHeader("mac.component.side-pane-header.history", .history),
    ]

    private static func sidePaneHeader(
        _ id: String,
        _ tab: SidePaneTab,
        lead: SidePaneLead = .none
    ) -> ShotScene {
        component(id, height: 50) {
            MacSidePaneHeader(
                tab: tab,
                onSelectTab: { _ in },
                lead: lead,
                onClose: {}
            )
            .frame(width: 380, alignment: .leading)
        }
    }
}
