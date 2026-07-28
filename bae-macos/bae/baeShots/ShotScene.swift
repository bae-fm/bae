@testable import bae
import BaeKit
import SwiftUI

/// One named UI scene to capture for the screenshot gallery: a stable id
/// shared across platforms by convention, the point size to render at, and a
/// builder for the composed production view. The builder returns the same
/// `PreviewScenes` composition its `#Preview` renders — capture and preview
/// share one path.
@MainActor
struct ShotScene {
    let id: String
    let size: CGSize
    let makeView: () -> AnyView

    /// Every macOS scene, in gallery order. Scene ids are desktop-story ids
    /// (notes/desktop-stories.md) — the gallery is the stories' per-platform
    /// verification sheet. A scene is present only when it has a real staging;
    /// a missing scene is a deliberate omission, never a swallowed failure.
    static let all: [ShotScene] = [
        ShotScene(id: "story-1-first-run", size: WelcomeWindow.size) {
            AnyView(PreviewScenes.welcome())
        },
        ShotScene(id: "story-3-empty-library", size: MainWindow.defaultSize) {
            AnyView(PreviewScenes.libraryEmpty())
        },
        ShotScene(id: "import-release-queue", size: CGSize(width: 900, height: 700)) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    store: PreviewData.releaseQueueImportStore,
                    tab: .ready,
                    collapseReadyGroup: false,
                    refreshingWatchedFolderPath: nil
                )
            )
        },
        ShotScene(
            id: "import-release-ambiguity-narrow",
            size: CGSize(width: 520, height: 700)
        ) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    store: PreviewData.releaseQueueImportStore,
                    tab: .needsYou,
                    collapseReadyGroup: false,
                    refreshingWatchedFolderPath: nil
                )
            )
        },
        ShotScene(
            id: "import-release-queue-collapsed",
            size: CGSize(width: 900, height: 700)
        ) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    store: PreviewData.releaseQueueImportStore,
                    tab: .ready,
                    collapseReadyGroup: true,
                    refreshingWatchedFolderPath: nil
                )
            )
        },
        ShotScene(
            id: "import-release-scanning-refresh",
            size: CGSize(width: 900, height: 700)
        ) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    store: PreviewData.releaseQueueScanningImportStore,
                    tab: .ready,
                    collapseReadyGroup: false,
                    refreshingWatchedFolderPath:
                    PreviewData.releaseQueueWatchedFolder.path
                )
            )
        },
        ShotScene(
            id: "import-release-resolved-reversal",
            size: CGSize(width: 900, height: 700)
        ) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    store: PreviewData.releaseQueueResolvedImportStore,
                    tab: .ready,
                    collapseReadyGroup: false,
                    refreshingWatchedFolderPath: nil
                )
            )
        },
    ]
}
