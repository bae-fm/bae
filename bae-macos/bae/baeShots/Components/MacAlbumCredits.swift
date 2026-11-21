import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacAlbumCredits`, one shot for each row of its preview.
    static let macAlbumCreditsScenes: [ShotScene] = [
        albumCredits(
            "mac.component.album-credits.three-credits",
            height: 160,
            [
                credit("Producer", "Ines Varga"),
                credit("Engineer", "Pell Marsh"),
                credit("Arranger", "Ada Lindqvist", tracks: ["2", "5"]),
            ]
        ),
        albumCredits(
            "mac.component.album-credits.one-credit",
            height: 110,
            [credit("Producer", "Ines Varga")]
        ),
    ]

    private static func credit(
        _ role: String,
        _ artist: String,
        tracks: [String] = []
    ) -> BridgeCreditRow {
        BridgeCreditRow(
            role: .word(role),
            artists: .named(artist),
            tracks: tracks
        )
    }

    private static func albumCredits(
        _ id: String,
        height: CGFloat,
        _ rows: [BridgeCreditRow]
    ) -> ShotScene {
        component(id, width: 480, height: height) {
            MacAlbumCredits(credits: rows)
                .frame(width: 480, alignment: .leading)
        }
    }
}
