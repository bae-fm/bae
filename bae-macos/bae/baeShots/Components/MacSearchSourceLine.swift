import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacSearchSourceLine`, a catalog a typed search still waits on.
    static let macSearchSourceLineScenes: [ShotScene] = [
        component("mac.component.search-source-line.searching", height: 40) {
            MacSearchSourceLine(
                catalog: "Discogs",
                status: .searching,
                onRetry: {}
            )
        },
        component("mac.component.search-source-line.failed", height: 40) {
            MacSearchSourceLine(
                catalog: "Discogs",
                status: .failed(reason: "Discogs did not answer in time."),
                onRetry: {}
            )
        },
        component(
            "mac.component.search-source-line.another-catalog",
            height: 40
        ) {
            MacSearchSourceLine(
                catalog: "MusicBrainz",
                status: .searching,
                onRetry: {}
            )
        },
    ]
}
