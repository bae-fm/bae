import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The import list's boards that need no folders behind them: the list
    /// that could not be read, with a failure that can be tried again.
    static let importListScenes: [ShotScene] = [
        ShotScene(
            id: "mac.import.list.failed",
            size: CGSize(width: 600, height: 400)
        ) {
            ShotStage(
                ImportListFailedView(
                    failure: DisplayError(
                        line: "Could not read the import list.",
                        detail: "database disk image is malformed"
                    ),
                    onRetry: {}
                )
            )
        }
    ]
}
