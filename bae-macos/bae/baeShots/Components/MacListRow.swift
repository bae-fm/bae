import AppKit
import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacListRow`, a playlist's track: at rest, with a person at its end,
    /// left faint and raised while the owner drags it, with a taller cover, and
    /// with its number hung in the margin.
    static let macListRowScenes: [ShotScene] = [
        listRow("mac.component.list-row.resting"),
        listRow("mac.component.list-row.who", who: true, wide: true),
        listRow("mac.component.list-row.lifted", lift: .lifted),
        listRow("mac.component.list-row.dragging", lift: .dragging),
        listRow("mac.component.list-row.cover", coverSize: 40),
        listRow(
            "mac.component.list-row.hanging",
            hanging: true,
            coverSize: 40,
            padsMargin: true
        ),
    ]

    private static func listRow(
        _ id: String,
        who: Bool = false,
        wide: Bool = false,
        lift: MacListRow.Lift = .resting,
        hanging: Bool = false,
        coverSize: CGFloat = 32,
        padsMargin: Bool = false
    ) -> ShotScene {
        component(id, height: 70, images: coverStore()) {
            MacListRow(
                lead: "21:04",
                leadWidth: 56,
                hanging: hanging,
                coverSize: coverSize,
                cover: cover("Low Tide Hours"),
                title: "Harbor Lights",
                subtitle: "Mara Quill · Low Tide Hours",
                who: who
                    ? BridgePersonRef(
                        person: "person-ana",
                        name: "Ana",
                        initials: "A",
                        avatarVersion: nil
                    ) : nil,
                duration: "4:32",
                wide: wide,
                lift: lift
            )
            .padding(
                .leading,
                padsMargin ? MacListRow.hang(leadWidth: 56) : 0
            )
            .frame(width: 380, alignment: .leading)
        }
    }
}
