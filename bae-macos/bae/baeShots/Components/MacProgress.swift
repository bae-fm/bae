import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacProgress`, a step's name over its bar. Its Named only row, a bar with no words, is the design's only.
    static let macProgressScenes: [ShotScene] = [
        component(
            "mac.component.progress.length-unknown",
            width: 842,
            height: 28
        ) {
            MacProgress(title: "Uploading…", fraction: nil)
                .frame(width: 842, alignment: .leading)
        },
        component("mac.component.progress.length-known", width: 842, height: 28)
        {
            MacProgress(title: "Uploading…", fraction: 0.3)
                .frame(width: 842, alignment: .leading)
        },
    ]
}

extension MacProgress where Bar == ProgressTrackBar {
    /// A step whose bar fills to `fraction`: 0...1 for a determinate fill,
    /// nil for the marching pill.
    init(title: String, fraction: Double?) {
        self.init(title: title, bar: ProgressTrackBar(progress: fraction))
    }
}
