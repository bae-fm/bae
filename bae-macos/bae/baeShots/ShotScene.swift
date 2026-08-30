import BaeKit
import SwiftUI

@testable import bae

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
        ShotScene(
            id: "import-release-queue",
            size: CGSize(width: 900, height: 700)
        ) {
            AnyView(
                PreviewScenes.importReleaseQueue(
                    scene: PreviewData.releaseQueueScene(),
                    tab: .pending,
                    collapsePendingGroup: false,
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
                    scene: PreviewData.releaseQueueScene(),
                    tab: .pending,
                    collapsePendingGroup: false,
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
                    scene: PreviewData.releaseQueueScene(),
                    tab: .pending,
                    collapsePendingGroup: true,
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
                    scene: PreviewData.releaseQueueScanningScene(),
                    tab: .pending,
                    collapsePendingGroup: false,
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
                    scene: PreviewData.releaseQueueResolvedScene(),
                    tab: .pending,
                    collapsePendingGroup: false,
                    refreshingWatchedFolderPath: nil
                )
            )
        },
        ShotScene(
            id: "storage-manager-dense",
            size: CGSize(width: 1_440, height: 900)
        ) {
            AnyView(
                StorageManagerPreviewScene(selectedReleaseId: "rel-row-1")
            )
        },
        ShotScene(
            id: "storage-manager-empty",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(StorageManagerPreviewScene(rows: []))
        },
        ShotScene(
            id: "storage-manager-empty-ish",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2))
                )
            )
        },
        ShotScene(
            id: "storage-manager-empty-ish-inspector",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2)),
                    inspectorPresented: true
                )
            )
        },
        ShotScene(
            id: "storage-manager-one-sync-inspector",
            size: CGSize(width: 940, height: 600)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    rows: Array(PreviewData.storageRows.prefix(2)),
                    selectedReleaseId: "rel-row-2",
                    inspectorPresented: true,
                    inspectorTab: .transfers,
                    downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                    outputSnapshot: PreviewData.emptyOutputSnapshot,
                    outboxSnapshot: PreviewData.outboxSnapshot(
                        uploadGroups: [PreviewData.uploadGroupDone],
                        deletes: []
                    )
                )
            )
        },
    ]
}
