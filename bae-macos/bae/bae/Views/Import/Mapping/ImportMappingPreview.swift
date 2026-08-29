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
            previewingPath: String? = nil,
            runtime: BridgeCandidateRuntimeSnapshot? = nil
        ) -> some View {
            let store = ImportStore()
            store.selectedCandidates[candidate.key] = candidate
            return ImportMappingPane(
                candidate: candidate,
                runtime: runtime,
                bindingOptions: PreviewData.sheetBindingOptions,
                previewingPath: previewingPath,
                libraryStatus: nil,
                hasCoverOptions: true,
                coverContent: nil,
                editActions: ReleaseFieldWriter { _, _ in },
                storageCloud: storageCloud,
                storagePinned: storagePinned,
                mappingActions: inertMappingActions(),
                commitActions: ImportCommitActions(
                    confirmImport: {},
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
            previewingPath: "/tmp/fake/Track 3.flac"
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
        @Previewable
        @State
        var storageCloud = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.sheetMappingCandidate,
            storageCloud: $storageCloud,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
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
