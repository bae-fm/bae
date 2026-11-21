import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacProgressLine`, a phase, a bar and a count on one line.
    static let macProgressLineScenes: [ShotScene] = [
        component("mac.component.progress-line.with-a-count", height: 50) {
            MacProgressLine(
                "Identifying",
                progress: 112 / 130,
                detail: "112 / 130"
            )
            .themeText(.body)
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.progress-line.no-count", height: 50) {
            MacProgressLine("Reading files…", progress: 0.42)
                .themeText(.body)
                .frame(width: 380, alignment: .leading)
        },
        component("mac.component.progress-line.no-known-length", height: 50) {
            MacProgressLine("Importing…", progress: nil)
                .themeText(.body)
                .frame(width: 380, alignment: .leading)
        },
        component("mac.component.progress-line.upload", height: 50) {
            MacProgressLine(
                "Uploading",
                bar: ProgressTrackBar(progress: 0.15)
            )
            .themeText(.body)
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.progress-line.long-phase", height: 50) {
            MacProgressLine(
                "A phase named at such length that it has to yield to the bar",
                progress: 0.6
            )
            .themeText(.body)
            .frame(width: 200)
        },
    ]
}
