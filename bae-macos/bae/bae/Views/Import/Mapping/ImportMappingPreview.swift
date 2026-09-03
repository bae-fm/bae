#if DEBUG
    import BaeKit
    import SwiftUI

    /// The stores every import preview reads plus the app's window background,
    /// injected as one modifier: ImageStore + UiStore for the search pane and
    /// the lightbox, Library for artist search, OutboxStore + ConfigStore for
    /// the commit bar, and `windowBackground()` so the preview reproduces the
    /// shell the panes are transparent over.
    extension View {
        func importPreviewEnvironment() -> some View {
            self
                .environment(PreviewData.artImageStore())
                .environment(UiStore())
                .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
                .environment(PreviewData.configStore())
                .environment(Library.stub())
                .environment(Playback.stub())
                .environment(PreviewData.releaseEditor())
                .environment(SettingsNavigation())
                .windowBackground()
        }
    }

    /// Preview builder for the mapping pane — fixes the callbacks and the
    /// stores, exposing only the candidate and the editor a permutation varies.
    enum ImportMappingPreview {
        @MainActor
        static func make(
            candidate: Candidate,
            storageCloud: Binding<Bool>,
            storagePinned: Binding<Bool>,
            previewingTarget: BridgePreviewTarget? = nil,
            runtime: BridgeCandidateRuntimeSnapshot? = nil
        ) -> some View {
            let store = ImportStore()
            store.selectedCandidates[candidate.key] = candidate
            return ImportMappingPane(
                candidate: candidate,
                runtime: runtime,
                bindingOptions: PreviewData.sheetBindingOptions,
                previewingTarget: previewingTarget,
                libraryStatus: nil,
                hasCoverOptions: true,
                coverContent: nil,
                editActions: ReleaseFieldWriter { _, _ in },
                editingCommands: EditingCommitCommands(),
                endEditing: {},
                storageCloud: storageCloud,
                storagePinned: storagePinned,
                mappingActions: inertMappingActions(),
                commitActions: ImportCommitActions(
                    confirmImport: {},
                    mergeArtists: { _ in },
                    viewInLibrary: { _ in },
                ),
                onPresentMetadata: { _ in },
                onReadFileTags: {},
                onUseFileTags: {},
                onClearMetadata: {},
                onEditCover: {},
                onSelectCover: { _ in },
                onNavigateToPlacement: { _ in },
            )
            .environment(store)
            .environment(PreviewData.importTabImporter())
        }
    }

    /// Every control wired to nothing, so a preview renders the real rows
    /// without a store behind them.
    func inertMappingActions() -> ImportMappingActions {
        ImportMappingActions(
            setRole: { _, _ in },
            bindSheet: { _, _ in },
            setSheetDisc: { _, _ in },
            openDocument: { _, _ in },
            openImages: { _, _ in },
            preview: { _ in },
            stopPreview: {},
            editTrack: { _ in },
            setTrackArtists: { _, _ in },
            chooseFile: { _, _ in },
            drop: { _ in },
            exclude: { _ in },
        )
    }

    /// The production CUE mapping pane used by both Xcode previews and the
    /// screenshot target, with only its viewport width varying.
    @MainActor
    struct ImportMappingCuePreviewScene: View {
        let width: CGFloat

        @State
        private var storageCloud = true
        @State
        private var storagePinned = true

        var body: some View {
            ImportMappingPreview.make(
                candidate: PreviewData.sheetMappingCandidate,
                storageCloud: $storageCloud,
                storagePinned: $storagePinned
            )
            .frame(width: width, height: 900)
            .importPreviewEnvironment()
        }
    }

    #Preview("Mapping pane — blank draft") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.blankDraftMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — a release picked") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.mappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned,
            previewingTarget: BridgePreviewTarget(
                path: "/tmp/fake/Track 3.flac",
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
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.discogsMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — Lookup, no search") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unidentifiedMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — File tags, before read") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unreadFileTagsMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — File tags read, not selected") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unidentifiedFileTagsMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — File tags loading") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.loadingFileTagsMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — direct entry") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.directEntryMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — several matches to pick") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.severalMatchesMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — a cue carving one container") {
        ImportMappingCuePreviewScene(width: 1212)
    }

    #Preview("Mapping pane — a cue carving one container, narrow") {
        ImportMappingCuePreviewScene(width: 760)
    }

    #Preview("Mapping pane — one file for ten tracks") {
        @Previewable
        @State
        var values = PreviewData.moreTracksEditValues
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — one file for ten tracks, narrow") {
        @Previewable
        @State
        var values = PreviewData.moreTracksEditValues
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 760, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — File tags selected") {
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.fileTagsMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }
#endif
