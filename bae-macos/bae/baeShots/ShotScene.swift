import BaeKit
import SwiftUI

@testable import bae

/// One named UI scene to capture for the screenshot gallery: a stable id
/// shared across platforms by convention, the point size to render at, and a
/// builder for the composed production view and what it waits for. The
/// builder returns the same `PreviewScenes` composition its `#Preview`
/// renders — capture and preview share one path.
@MainActor
struct ShotScene {
    let id: String
    /// The view's size, in points; a scene shown in its window adds the
    /// window's title bar.
    let size: CGSize
    var frame: ShotFrame = .content
    let stage: () -> ShotStage

    /// Every macOS scene, in gallery order. Scene ids are desktop-story ids
    /// (notes/desktop-stories.md) — the gallery is the stories' per-platform
    /// verification sheet. A scene is present only when it has a real staging;
    /// a missing scene is a deliberate omission, never a swallowed failure.
    static let all: [ShotScene] =
        [
            importMetadataDraft,
            importSearchResults,
            welcomeLibraries,
            ShotScene(id: "appearance", size: CGSize(width: 650, height: 400)) {
                ShotStage(AppearanceSettingsTab())
            },
            ShotScene(
                id: "artwork-lightbox",
                size: CGSize(width: 1_148, height: 868)
            ) {
                let load = LightboxImageLoad()
                let images = PreviewData.artImageStore()
                return ShotStage(
                    CoverPickerPreviewScene.lightbox(
                        load: load,
                        images: images
                    ),
                    images: images
                ) { load.ended() }
            },
            ShotScene(
                id: "cover-picker-wide",
                size: CGSize(width: 1_148, height: 868)
            ) {
                coverPicker()
            },
            ShotScene(
                id: "cover-picker-short",
                size: CGSize(width: 800, height: 520)
            ) {
                coverPicker()
            },
            ShotScene(id: "story-1-first-run", size: WelcomeWindow.size) {
                ShotStage(PreviewScenes.welcome(initialMode: .choose))
            },
            ShotScene(
                id: "cover-picker-unlinked",
                size: CGSize(width: 1_148, height: 868)
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    CoverPickerPreviewScene.unlinked(images: images),
                    images: images
                )
            },
            ShotScene(
                id: "album-detail",
                size: CGSize(width: 1100, height: 800)
            ) {
                ShotStage(PreviewScenes.albumDetail())
            },
            ShotScene(
                id: "artist-pane-first-read",
                size: CGSize(width: 520, height: 640)
            ) {
                let uiStore = UiStore()
                let session = PreviewData.browseSession(uiStore: uiStore)
                session.selectArtist("artist-0")
                let images = ImageStore.stub()
                return ShotStage(
                    ArtistDetailPane(detail: nil)
                        .environment(session)
                        .environment(uiStore)
                        .environment(ArtistEditor())
                        .environment(images),
                    images: images
                )
            },
            settingsImport("settings-import-key-stored", discogsUsable: true),
            settingsImport("settings-import-no-key", discogsUsable: false),
            ShotScene(
                id: "album-detail-mixed-columns",
                size: CGSize(width: 1100, height: 800)
            ) {
                ShotStage(PreviewScenes.albumDetailMixedColumns())
            },
            emptyStateWithoutKey(
                "library-empty-no-key",
                LibraryEmptyState(title: "No albums")
            ),
            emptyStateWithoutKey("import-empty-no-key", ImportEmptyState()),
            ShotScene(id: "story-3-empty-library", size: MainWindow.defaultSize)
            {
                let uiStore = UiStore()
                let backing = LibraryView.previewEmptyBacking(uiStore: uiStore)
                // The empty state shows once the album list has counted.
                return ShotStage(
                    PreviewScenes.libraryEmpty(
                        uiStore: uiStore,
                        backing: backing
                    )
                ) { backing.session.albumTotal != nil }
            },
            importReleaseQueue(
                "import-release-queue",
                width: 900,
                scene: PreviewData.releaseQueueScene,
                collapsePendingGroup: false,
                refreshingWatchedFolderPath: nil
            ),
            importReleaseQueue(
                "import-release-ambiguity-narrow",
                width: 520,
                scene: PreviewData.releaseQueueScene,
                collapsePendingGroup: false,
                refreshingWatchedFolderPath: nil
            ),
            importReleaseQueue(
                "import-release-queue-collapsed",
                width: 900,
                scene: PreviewData.releaseQueueScene,
                collapsePendingGroup: true,
                refreshingWatchedFolderPath: nil
            ),
            importReleaseQueue(
                "import-release-scanning-refresh",
                width: 900,
                scene: PreviewData.releaseQueueScanningScene,
                collapsePendingGroup: false,
                refreshingWatchedFolderPath: PreviewData
                    .releaseQueueWatchedFolder.path
            ),
            importReleaseQueue(
                "import-release-resolved-reversal",
                width: 900,
                scene: PreviewData.releaseQueueResolvedScene,
                collapsePendingGroup: false,
                refreshingWatchedFolderPath: nil
            ),
            ShotScene(
                id: "import-bulk-selection",
                size: CGSize(width: 936, height: 696)
            ) {
                ShotStage(PreviewScenes.importBulkSelection())
            },
            ShotScene(
                id: "import-done-rows-uploading",
                size: CGSize(width: 360, height: 240)
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    PreviewScenes.importDoneRowsUploading(images: images),
                    images: images
                )
            },
            ShotScene(
                id: "import-done-rows-settled",
                size: CGSize(
                    width: 360,
                    height: PreviewScenes.triageRowsHeight(rowCount: 2)
                )
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    PreviewScenes.importDoneRowsSettled(images: images),
                    images: images
                )
            },
            ShotScene(
                id: "import-failed-row",
                size: CGSize(
                    width: 360,
                    height: PreviewScenes.triageRowsHeight(rowCount: 1)
                )
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    PreviewScenes.importFailedRow(images: images),
                    images: images
                )
            },
            ShotScene(
                id: "import-mapping-cue-wide",
                size: CGSize(width: 1212, height: 900)
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    ImportMappingCuePreviewScene(width: 1212, images: images),
                    images: images
                )
            },
            ShotScene(
                id: "import-mapping-cue-narrow",
                size: CGSize(width: 760, height: 900)
            ) {
                let images = PreviewData.artImageStore()
                return ShotStage(
                    ImportMappingCuePreviewScene(width: 760, images: images),
                    images: images
                )
            },
            ShotScene(
                id: "queue-pane-standard",
                size: CGSize(width: 420, height: 720)
            ) {
                let fixture = QueueViewPreviewScene.fixture(for: .populated)
                let images = ImageStore.stub()
                return ShotStage(
                    QueueViewPreviewScene(width: 420)
                        .environment(fixture.store)
                        .environment(fixture.queue)
                        .environment(images),
                    images: images
                )
            },
            ShotScene(
                id: "queue-pane-narrow",
                size: CGSize(width: 320, height: 720)
            ) {
                let fixture = QueueViewPreviewScene.fixture(for: .populated)
                let images = ImageStore.stub()
                return ShotStage(
                    QueueViewPreviewScene(width: 320)
                        .environment(fixture.store)
                        .environment(fixture.queue)
                        .environment(images),
                    images: images
                )
            },
            ShotScene(
                id: "player-waiting-on-storage",
                size: CGSize(width: 1100, height: 100)
            ) {
                ShotStage(PreviewScenes.nowPlayingBar(.waitingOnStorage))
            },
            ShotScene(
                id: "player-playing-with-a-gap",
                size: CGSize(width: 1100, height: 100)
            ) {
                ShotStage(PreviewScenes.nowPlayingBar(.playingWithAGap))
            },
            ShotScene(
                id: "player-all-here",
                size: CGSize(width: 1100, height: 100)
            ) {
                ShotStage(PreviewScenes.nowPlayingBar(.allHere))
            },
        ] + storageManagerScenes + waitScenes + findOnlineScenes
        + libraryScenes + importRowScenes + coverPickerScenes + releaseScenes
        + settingsScenes
        + syncSetupScenes
        + sheetFooterScenes
        + restoreCodeScenes
}

extension ShotScene {
    private static let importMetadataDraft = ShotScene(
        id: "import-metadata-draft",
        size: CGSize(width: 800, height: 700)
    ) {
        let images = PreviewData.artImageStore()
        return ShotStage(
            ImportMappingPreview.make(
                candidate: PreviewData.mappingCandidate,
                storageCloud: .constant(true),
                storagePinned: .constant(true)
            )
            .importPreviewEnvironment(images: images),
            images: images
        )
    }

    private static let welcomeLibraries = ShotScene(
        id: "welcome-libraries",
        size: CGSize(width: 460, height: 190)
    ) {
        ShotStage(PreviewScenes.welcomeLibraries())
    }

    private static let importSearchResults = ShotScene(
        id: "import-search-results",
        size: CGSize(width: 800, height: 520)
    ) {
        let images = PreviewData.artImageStore()
        return ShotStage(
            FindOnlineSearchResults(
                search: PreviewData.manualSearchRun,
                isImporting: false,
                libraryStatuses: [:],
                selectedReleaseId: nil,
                loadingPick: nil,
                onRetry: {},
                onSelect: { _ in },
            )
            .importPreviewEnvironment(images: images),
            images: images
        )
    }
}

extension ShotScene {
    /// The import list's pending tab over what `scene` builds, `width`
    /// points wide, its first group folded when `collapsePendingGroup`, and
    /// the watched folder at `refreshingWatchedFolderPath` refreshing.
    fileprivate static func importReleaseQueue(
        _ id: String,
        width: CGFloat,
        scene: @escaping @MainActor () -> ImportPreviewFixture,
        collapsePendingGroup: Bool,
        refreshingWatchedFolderPath: Data?
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: width, height: 700)) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                PreviewScenes.importReleaseQueue(
                    scene: scene(),
                    tab: .pending,
                    collapsePendingGroup: collapsePendingGroup,
                    refreshingWatchedFolderPath: refreshingWatchedFolderPath,
                    images: images
                ),
                images: images
            )
        }
    }

    /// The cover picker over decoded fixture artwork.
    fileprivate static func coverPicker() -> ShotStage {
        let images = PreviewData.artImageStore()
        return ShotStage(
            CoverPickerPreviewScene(images: images),
            images: images
        )
    }

    /// Settings' Import pane, with a stored Discogs key or none.
    fileprivate static func settingsImport(
        _ id: String,
        discogsUsable: Bool
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 700, height: 620)) {
            ShotStage(
                ImportSettingsTab()
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: discogsUsable
                        )
                    )
                    .environment(Discogs.stub())
                    .environment(Importer.stub())
                    .environment(UiStore())
            )
        }
    }

    /// An empty state over an empty library with no usable Discogs key,
    /// which offers Add Discogs Key.
    fileprivate static func emptyStateWithoutKey(
        _ id: String,
        _ content: some View
    ) -> ShotScene {
        ShotScene(id: id, size: CGSize(width: 600, height: 400)) {
            let uiStore = UiStore()
            return ShotStage(
                content
                    .environment(uiStore)
                    .environment(
                        PreviewData.countedBrowseSession(
                            albumCount: 0,
                            uiStore: uiStore
                        )
                    )
                    .environment(
                        PreviewData.makeConfigStore(
                            libraryFullWidth: false,
                            discogsUsable: false
                        )
                    )
                    .environment(SettingsNavigation())
                    .windowBackground()
            )
        }
    }
}

/// A scene's view, built with its fixture, and whether what the view shows
/// has arrived: the reads it makes off the main thread answered and taken
/// up, an image decoded. The scene knows which reads it needs, so it says
/// when they are done; a scene whose views read nothing off the main thread
/// is ready at once.
@MainActor
struct ShotStage {
    let view: AnyView
    /// Read under observation tracking, so it holds only what a store
    /// publishes.
    let isReady: @MainActor () -> Bool

    init(
        _ view: some View,
        isReady: @escaping @MainActor () -> Bool = { true }
    ) {
        self.view = AnyView(view)
        self.isReady = isReady
    }

    /// `view`, whose art comes from `images`, ready once the store has
    /// handed back every image asked of it, decoded or failed, and
    /// `isReady` holds. The store hands an image back a main-actor step
    /// before the view that asked shows it, and a view asks a step after it
    /// is first drawn; the tree's coming to rest spans both.
    init(
        _ view: some View,
        images: ImageStore,
        isReady: @escaping @MainActor () -> Bool = { true }
    ) {
        self.init(view) { images.unansweredImages == 0 && isReady() }
    }
}

/// What a shot takes in: the scene's view alone, or the window it is shown
/// in, title bar and toolbar included, for a scene whose toolbar is part of
/// what it shows.
enum ShotFrame {
    case content
    case window(title: String)
}
