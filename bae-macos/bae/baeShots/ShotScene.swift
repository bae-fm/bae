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
        importMetadataDraft,
        importSearchResults,
        ShotScene(
            id: "import-combine-folders",
            size: CGSize(width: 936, height: 696)
        ) {
            AnyView(ImportCombinationPreviewScene())
        },
        ShotScene(id: "appearance", size: CGSize(width: 650, height: 400)) {
            AnyView(AppearanceSettingsTab())
        },
        ShotScene(
            id: "artwork-lightbox",
            size: CGSize(width: 1_148, height: 868)
        ) {
            AnyView(CoverPickerPreviewScene.lightbox())
        },
        ShotScene(
            id: "cover-picker-wide",
            size: CGSize(width: 1_148, height: 868)
        ) {
            AnyView(CoverPickerPreviewScene())
        },
        ShotScene(
            id: "cover-picker-short",
            size: CGSize(width: 800, height: 520)
        ) {
            AnyView(CoverPickerPreviewScene())
        },
        ShotScene(id: "story-1-first-run", size: WelcomeWindow.size) {
            AnyView(PreviewScenes.welcome())
        },
        ShotScene(
            id: "cover-picker-unlinked",
            size: CGSize(width: 1_148, height: 868)
        ) {
            AnyView(CoverPickerPreviewScene.unlinked())
        },
        ShotScene(id: "album-detail", size: CGSize(width: 1100, height: 800)) {
            AnyView(PreviewScenes.albumDetail())
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
            id: "import-mapping-cue-wide",
            size: CGSize(width: 1212, height: 900)
        ) {
            AnyView(ImportMappingCuePreviewScene(width: 1212))
        },
        ShotScene(
            id: "import-mapping-cue-narrow",
            size: CGSize(width: 760, height: 900)
        ) {
            AnyView(ImportMappingCuePreviewScene(width: 760))
        },
        ShotScene(
            id: "queue-pane-standard",
            size: CGSize(width: 420, height: 720)
        ) {
            AnyView(
                QueueViewPreviewScene(width: 420)
                    .environment(
                        QueueViewPreviewScene.store(for: .populated)
                    )
                    .environment(Queue.stub())
                    .environment(ImageStore.stub())
            )
        },
        ShotScene(
            id: "queue-pane-narrow",
            size: CGSize(width: 320, height: 720)
        ) {
            AnyView(
                QueueViewPreviewScene(width: 320)
                    .environment(
                        QueueViewPreviewScene.store(for: .populated)
                    )
                    .environment(Queue.stub())
                    .environment(ImageStore.stub())
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
            id: "storage-manager-active-file-inspector",
            size: CGSize(width: 940, height: 700)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                    outputSnapshot: PreviewData.emptyOutputSnapshot
                )
            )
        },
        ShotScene(
            id: "storage-manager-idle-file-inspector",
            size: CGSize(width: 940, height: 700)
        ) {
            AnyView(
                StorageManagerPreviewScene(
                    selectedReleaseId: "rel-row-1",
                    inspectorPresented: true,
                    downloadSnapshot: PreviewData.emptyDownloadSnapshot,
                    outputSnapshot: PreviewData.emptyOutputSnapshot,
                    outboxSnapshot: PreviewData.outboxSnapshot(
                        uploadGroups: [],
                        deletes: []
                    )
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

extension ShotScene {
    private static let importMetadataDraft = ShotScene(
        id: "import-metadata-draft",
        size: CGSize(width: 800, height: 700)
    ) {
        AnyView(
            ImportMappingPreview.make(
                candidate: PreviewData.mappingCandidate,
                storageCloud: .constant(true),
                storagePinned: .constant(true)
            )
            .importPreviewEnvironment()
        )
    }

    private static let importSearchResults = ShotScene(
        id: "import-search-results",
        size: CGSize(width: 800, height: 520)
    ) {
        AnyView(
            FindOnlineSearchResults(
                search: PreviewData.manualSearchRun,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingReleaseId: nil,
                onClear: {},
                onRetry: {},
                onOpenSettings: {},
                onSelect: { _ in },
                onSourceSearch: { _, _ in }
            )
            .importPreviewEnvironment()
        )
    }
}
