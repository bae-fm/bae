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

#endif
