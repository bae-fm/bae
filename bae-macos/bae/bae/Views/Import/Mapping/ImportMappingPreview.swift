#if DEBUG
    import BaeKit
    import SwiftUI

    /// The stores every import preview reads plus the app's window background,
    /// injected as one modifier: ImageStore + UiStore for the search pane and
    /// the lightbox, ImportSelection for the list and the pane a selection
    /// opens, Library for artist search, OutboxStore + ConfigStore for
    /// the commit bar, Importer for the Find online header's source
    /// checkboxes, the primary window's values the Import tab reads (Settings
    /// for the metadata sources), and `windowBackground()` so the preview
    /// reproduces the shell the panes are transparent over.
    extension View {
        func importPreviewEnvironment() -> some View {
            self
                .environment(PreviewData.artImageStore())
                .importPreviewStores()
        }

        /// [`importPreviewEnvironment()`] with its images from `images`, for
        /// a screenshot that waits until they are there.
        func importPreviewEnvironment(images: ImageStore) -> some View {
            self
                .environment(images)
                .importPreviewStores()
        }

        /// Every store of [`importPreviewEnvironment()`] but its images. The
        /// images are injected where the modifier names them, so the
        /// environment audit can read their type off the call it sees.
        private func importPreviewStores() -> some View {
            self
                .environment(UiStore())
                .environment(ImportSelection())
                .environment(OutboxStore(uploadsPaused: false))
                .environment(
                    PreviewData.configStore(storage: PreviewData.previewStorage)
                )
                .environment(Importer.stub())
                .environment(Library.stub())
                .environment(Playback.stub())
                .environment(PreviewData.releaseEditor())
                .environment(SettingsNavigation())
                .mainWindowPreviewEnvironment()
                .windowBackground()
        }
    }

    /// Preview builder for the mapping pane — fixes the callbacks and the
    /// stores, exposing only the candidate and the editor a permutation varies.
    enum ImportMappingPreview {
        @MainActor
        static func make(
            candidate: Candidate,
            importDestination: Binding<BridgeImportDestination>,
            previewingTarget: BridgePreviewTarget? = nil,
            libraryStatus: BridgeLibraryStatus? = nil,
            runtime: BridgeCandidateRuntimeSnapshot? = nil
        ) -> some View {
            let store = ImportStore()
            store.selectedCandidates[candidate.key] = candidate
            return ImportMappingPane(
                candidate: candidate,
                previewingTarget: previewingTarget,
                isApplyingMetadata: false,
                libraryStatus: libraryStatus,
                hasCoverOptions: true,
                coverContent: nil,
                editActions: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                endEditing: {},
                importDestination: importDestination,
                mappingActions: inertMappingActions(),
                commitActions: ImportCommitActions(
                    confirmImport: {},
                    viewInLibrary: { _ in },
                ),
                albumActions: DraftAlbumActions(
                    pick: { _ in },
                    search: { _ in [] },
                    viewInLibrary: { _ in }
                ),
                onMovePane: { _ in },
                onIdentify: {},
                onSearchForRelease: {},
                onResetToFileMetadata: {},
                onClearMetadata: {},
                onUnlink: {},
                onEditCover: {},
                onSelectCover: { _ in },
                onNavigateToPlacement: { _ in },
            )
            .environment(store)
            .environment(Importer.stub())
            // The pane's leaves read what is in flight by the candidate's key.
            .environment(
                \.heldCandidateRuntimes,
                runtime.map { [candidate.key: $0] } ?? [:]
            )
        }
    }

    /// Every control wired to nothing, so a preview renders the real rows
    /// without a store behind them.
    func inertMappingActions() -> ImportMappingActions {
        ImportMappingActions(
            bindSheet: { _, _, _ in },
            setSheetDisc: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _, _ in },
            preview: { _ in },
            stopPreview: {},
            editTrack: { _ in },
        )
    }

    /// The production CUE mapping pane used by both Xcode previews and the
    /// screenshot target, with only its viewport width varying.
    @MainActor
    struct ImportMappingCuePreviewScene: View {
        let width: CGFloat
        /// Where the pane's images come from.
        let images: ImageStore

        @State
        private var importDestination = BridgeImportDestination.cloudPinned

        var body: some View {
            ImportMappingPreview.make(
                candidate: PreviewData.sheetMappingCandidate,
                importDestination: $importDestination
            )
            .frame(width: width, height: 900)
            .importPreviewEnvironment(images: images)
        }
    }

    #Preview("Mapping pane — blank draft") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.blankDraftMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — a release picked") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.mappingCandidate,
            importDestination: $importDestination,
            previewingTarget: BridgePreviewTarget(
                path: URL(fileURLWithPath: "/tmp/fake/Track 3.flac").pathBytes,
                name: "Track 3.flac",
                startSample: 0,
                endSample: nil
            )
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — Discogs applied") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.discogsMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — both sources applied") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.pairedMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — Lookup, no search") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.unidentifiedMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — direct entry") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.directEntryMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — several matches to pick") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.severalMatchesMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — a cue carving one container") {
        ImportMappingCuePreviewScene(
            width: 1212,
            images: PreviewData.artImageStore()
        )
    }

    #Preview("Mapping pane — a cue carving one container, narrow") {
        ImportMappingCuePreviewScene(
            width: 760,
            images: PreviewData.artImageStore()
        )
    }

    #Preview("Mapping pane — CUE ignored") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — CUE ignored, narrow") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 760, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — File tags selected") {
        @Previewable
        @State
        var importDestination = BridgeImportDestination.cloudPinned
        ImportMappingPreview.make(
            candidate: PreviewData.fileMetadataMappingCandidate,
            importDestination: $importDestination
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }
#endif
