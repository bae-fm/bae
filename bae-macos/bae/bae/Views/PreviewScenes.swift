#if DEBUG
    import BaeKit
    import Foundation
    import SwiftUI

    /// The named UI scenes captured for the cross-platform screenshot gallery.
    /// Each builder composes production views over `PreviewData` fixtures; the
    /// matching `#Preview`s render these same builders, so the gallery captures
    /// and the previews render one path and can't drift.
    @MainActor
    enum PreviewScenes {
        /// The welcome window entered at `initialMode`, in its fixed 900x600
        /// chrome, over `setup` and the `LibraryRemovals` fixtures. Every flow
        /// can go back to the chooser, which reads both lists from `setup`.
        static func welcome(
            initialMode: WelcomeView.Mode,
            setup: LibrarySetup
        ) -> some View {
            WelcomeWindowChrome {
                WelcomeView(
                    onLibraryReady: { _, _ in },
                    onOpenLibrary: { _ in },
                    onFinishReplacement: { _, _ in },
                    initialMode: initialMode,
                    canDeleteActiveLibrary: true
                )
            }
            .environment(setup)
            .environment(PreviewData.welcomeRemovals())
        }

        /// What the details popover of a Dropbox library holds, its status
        /// read through `readStanding`.
        static func welcomeLibraryDetails(
            readStanding:
                @escaping @Sendable (String) async throws
                -> BridgeLibraryStanding
        ) -> some View {
            WelcomeLibraryDetails(
                library: PreviewData.welcomeDropboxLibrary,
                readStanding: readStanding
            )
            .windowBackground()
        }

        /// The screen that offers resetting this device, in the welcome
        /// window's chrome, saying `error`.
        static func deviceReset(error: LoadError) -> some View {
            WelcomeWindowChrome {
                DeviceResetView(
                    prompt: .offered,
                    error: error,
                    onReset: {},
                    onCancel: {}
                )
            }
        }

        /// The album grid backed by the fixture albums, wired with the same live
        /// stores and stub services the app injects at the main window root.
        static func libraryGrid() -> some View {
            let uiStore = UiStore()
            let backing = LibraryView.previewGridBacking(uiStore: uiStore)
            return LibraryView()
                .environment(ImageStore.stub())
                .environment(Playback.stub())
                .environment(Queue.stub())
                .environment(ReleaseEditor.stub())
                .environment(Downloads.stub())
                .environment(backing.library)
                .environment(
                    LibraryProjectionStore(library: backing.library)
                )
                .environment(uiStore)
                .environment(backing.session)
                .environment(PreviewData.configStore())
                .windowBackground()
        }

        /// The full main window over a library with nothing in it — title bar,
        /// empty albums state, idle now-playing bar (desktop story 3). Same
        /// composition as `MainAppView`'s preview, over the shared empty
        /// backing (`LibraryView.previewEmptyBacking`) made with `uiStore`,
        /// whose album count lands a read later.
        static func libraryEmpty(
            uiStore: UiStore,
            backing: (library: Library, session: LibraryBrowseSession)
        ) -> some View {
            let importScene = PreviewData.importTabScene()
            return MainAppView()
                .environment(backing.library)
                .environment(
                    LibraryProjectionStore(library: backing.library)
                )
                .environment(backing.session)
                .environment(uiStore)
                .environment(importScene.store)
                .environment(importScene.slot(uiStore: uiStore))
                .environment(PreviewAudio.stub())
                .environment(Cast.stub())
                .environment(CastStart.stub())
                .environment(CastStore())
                .environment(SyncActivityStore.preview(needsAttention: false))
                .albumDetailPreviewEnvironment()
                .environment(SettingsNavigation())
                .mainWindowPreviewEnvironment()
                .windowBackground()
        }

        /// Where the now-playing bar's track is, which parts of it are here,
        /// and whether its load waits on storage.
        enum NowPlayingLoad {
            /// The load waits on storage for its first sound, with the start
            /// of the track here.
            case waitingOnStorage
            /// Playing, with a gap a seek ahead left in what is here.
            case playingWithAGap
            /// Playing, with every byte of the track here.
            case allHere

            var progress: Double {
                switch self {
                case .waitingOnStorage: 0
                case .playingWithAGap: 0.3
                case .allHere: 0.55
                }
            }

            var held: [BridgeTrackSpan] {
                switch self {
                case .waitingOnStorage:
                    [BridgeTrackSpan(start: 0, end: 0.08)]
                case .playingWithAGap:
                    [
                        BridgeTrackSpan(start: 0, end: 0.45),
                        BridgeTrackSpan(start: 0.62, end: 0.85),
                    ]
                case .allHere:
                    [BridgeTrackSpan(start: 0, end: 1)]
                }
            }

            var waitsOnStorage: Bool { self == .waitingOnStorage }
        }

        /// The now-playing bar on a track in `load`.
        static func nowPlayingBar(_ load: NowPlayingLoad) -> some View {
            NowPlayingBarPreview(
                trackTitle: PreviewData.nowPlayingTitle,
                artistNames: PreviewData.nowPlayingArtist,
                isPlaying: true,
                held: load.held,
                waitsOnStorage: load.waitsOnStorage,
                repeatMode: .off,
                shuffled: false,
                progress: load.progress
            )
            .environment(ImageStore.stub())
            .environment(PreviewData.queueStore(manualCount: 2))
            .environment(Queue.stub())
            .windowBackground()
        }

        /// The expanded album detail for a two-sided vinyl fixture, mid-playback.
        static func albumDetail() -> some View {
            PreviewData.albumExpansionScene(
                albumId: "a-21",
                currentTrackId: "t-d2-3",
                isPlaying: true
            )
        }

        /// The expanded album detail for a two-disc CD fixture whose first
        /// disc splits into two columns and whose second is short enough to
        /// stay one — both column shapes in a single track list.
        static func albumDetailMixedColumns() -> some View {
            PreviewData.albumExpansionScene(albumId: "a-23")
        }

        static func importReleaseQueue(
            scene: ImportPreviewFixture,
            tab: BridgeTriageTab,
            collapsePendingGroup: Bool,
            refreshingWatchedFolderPath: Data?,
            images: ImageStore
        ) -> some View {
            let uiStore = UiStore()
            uiStore.setImportCandidateTab(tab)
            if let refreshingWatchedFolderPath {
                uiStore.setWatchedFolderRefreshing(
                    refreshingWatchedFolderPath,
                    true
                )
            }
            let scene = collapsePendingGroup ? scene.foldingFirstGroup() : scene
            return ImportCandidateListContent(
                importStore: scene.store,
                listSlot: scene.slot(uiStore: uiStore),
                onAddFolder: {},
                onRemoveFolder: { _ in },
                onRefreshFolder: { _ in },
                onCombineFolder: { _ in },
                onSeparate: { _ in },
                onShowInLibrary: { _ in },
                onPerform: { _ in },
                onCancelAllIdentification: {},
                onCancelAllImports: {}
            )
            .environment(OutboxStore(uploadsPaused: false))
            .environment(ImportSelection())
            .environment(images)
            .environment(uiStore)
            .mainWindowPreviewEnvironment()
            .windowBackground()
        }

        /// Done rows at the import list's row height: one at rest, one whose
        /// release's files wait to upload, and one whose release has a file
        /// being sent, its bar read from the release's upload stream.
        static func importDoneRowsUploading(images: ImageStore) -> some View {
            triageRows(
                [
                    PreviewData.triageRowDoneFromTags,
                    PreviewData.triageRowDoneUploadQueued,
                    PreviewData.triageRowDoneUploading,
                ],
                images: images
            )
        }

        /// Done rows at rest at the import list's row height: one whose
        /// release was read from the catalog and one read from tags.
        static func importDoneRowsSettled(images: ImageStore) -> some View {
            triageRows(
                [
                    PreviewData.triageRowDoneImported,
                    PreviewData.triageRowDoneFromTags,
                ],
                images: images
            )
        }

        /// A failed row at the import list's row height, with why it failed.
        static func importFailedRow(images: ImageStore) -> some View {
            triageRows([PreviewData.triageRowFailed], images: images)
        }

        /// The padding around the rows of a `triageRows` scene.
        static let triageRowsPadding: CGFloat = 16

        private static func triageRows(
            _ rows: [BridgeTriageRow],
            images: ImageStore
        ) -> some View {
            let importStore = ImportStore()
            return VStack(alignment: .leading, spacing: 0) {
                ForEach(rows, id: \.candidateKey) { row in
                    TriageRowView(
                        row: row,
                        coverContent: importStore.sidebarCover(for: row),
                        isGroupMember: false,
                        onShowInLibrary: {}
                    )
                    .frame(height: ImportCandidateTableLayout.rowHeight)
                }
            }
            .padding(triageRowsPadding)
            .frame(width: 360)
            .environment(PreviewData.doneRowOutboxStore())
            .environment(images)
            .candidateReaderPreviewEnvironment()
            .windowBackground()
        }

        /// The bulk-selection pane over two selected folders, one of them ready
        /// to import: every action the two offer, with how many folders each
        /// applies to.
        static func importBulkSelection() -> some View {
            let selection = PreviewData.importSelection(
                of: [
                    PreviewData.importTabCandidate.key,
                    PreviewData.importTabDisagreementCandidate.key,
                ],
                in: PreviewData.importTabScene().store
            )
            return ImportCandidateBulkSelectionPane(
                storageCloud: .constant(true),
                storagePinned: .constant(true),
                onPerform: { _ in }
            )
            .environment(selection)
            .environment(PreviewData.configStore())
            .windowBackground()
        }
    }

    #Preview("Import Release Queue") {
        PreviewScenes.importReleaseQueue(
            scene: PreviewData.releaseQueueScene(),
            tab: .pending,
            collapsePendingGroup: false,
            refreshingWatchedFolderPath: nil,
            images: PreviewData.artImageStore()
        )
    }

    #Preview("Import Release Queue Selected") {
        PreviewScenes.importReleaseQueue(
            scene: PreviewData.releaseQueueSelectedScene(),
            tab: .pending,
            collapsePendingGroup: false,
            refreshingWatchedFolderPath: nil,
            images: PreviewData.artImageStore()
        )
    }

    #Preview("Import Release Queue Scanning") {
        PreviewScenes.importReleaseQueue(
            scene: PreviewData.releaseQueueScanningScene(),
            tab: .pending,
            collapsePendingGroup: false,
            refreshingWatchedFolderPath:
                PreviewData.releaseQueueWatchedFolder.path,
            images: PreviewData.artImageStore()
        )
    }

    #Preview("Import Bulk Selection") {
        PreviewScenes.importBulkSelection()
    }

    #Preview("Import Release Queue Resolved") {
        PreviewScenes.importReleaseQueue(
            scene: PreviewData.releaseQueueResolvedScene(),
            tab: .pending,
            collapsePendingGroup: false,
            refreshingWatchedFolderPath: nil,
            images: PreviewData.artImageStore()
        )
    }
#endif
