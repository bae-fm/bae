import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let macSourcedValueScenes: [ShotScene] = [
        sourcedValueScene(
            "mac.component.sourced-value.one-record",
            [StatedValue(value: "1992", catalogs: [])]
        ),
        sourcedValueScene(
            "mac.component.sourced-value.records-differ",
            [
                StatedValue(value: "1992", catalogs: [.musicBrainz]),
                StatedValue(value: "1993", catalogs: [.discogs]),
            ],
            strong: true
        ),
    ]

    private static func sourcedValueScene(
        _ id: String,
        _ values: [StatedValue],
        strong: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 284,
            height: 40
        ) {
            if strong {
                MacSourcedValue(values)
                    .themeText(.strong)
                    .monospacedDigit()
            }
            else {
                MacSourcedValue(values)
            }
        }
    }
}
