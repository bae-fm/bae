import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Files section of a release, as a Found folder draws it (its track
    /// sheet still bindable) and as an imported release's pane draws it.
    static let releaseFilesScenes: [ShotScene] = [
        ShotScene(
            id: "release-files-found",
            size: CGSize(width: 560, height: 300)
        ) {
            ShotStage(
                ReleaseFilesView(
                    rows: PreviewData.releaseFilesRows,
                    onOpenDocument: { _, _ in },
                    sheetActions: inertMappingActions()
                )
                .padding(ThemeSpace.section)
                .background(Theme.background)
            )
        },
        ShotScene(
            id: "release-files-imported",
            size: CGSize(width: 560, height: 300)
        ) {
            ShotStage(
                ReleaseFilesView(
                    rows: PreviewData.releaseFilesRows,
                    onOpenDocument: { _, _ in }
                )
                .padding(ThemeSpace.section)
                .background(Theme.background)
            )
        },
    ]
}
