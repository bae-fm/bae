#if DEBUG
    import BaeKit
    import SwiftUI

    /// The stores every import preview reads plus the app's window background,
    /// injected as one modifier: ImageStore + UiStore for the search pane and
    /// the lightbox, OutboxStore + ConfigStore for the commit bar, and
    /// `windowBackground()` so the preview reproduces the shell the panes are
    /// transparent over.
    extension View {
        func importPreviewEnvironment() -> some View {
            self
                .environment(PreviewData.artImageStore())
                .environment(UiStore())
                .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
                .environment(PreviewData.configStore())
                .windowBackground()
        }
    }

    /// Preview builder for the mapping pane — fixes the callbacks and the
    /// stores, exposing only the candidate and the editor a permutation varies.
    enum ImportMappingPreview {
        @MainActor
        static func make(
            candidate: Candidate,
            editor: Binding<BridgeRawReleaseEdit>?,
            storageManaged: Binding<Bool>,
            storagePinned: Binding<Bool>,
            previewingPath: String? = nil
        ) -> some View {
            ImportMappingPane(
                candidate: candidate,
                bindingOptions: PreviewData.sheetBindingOptions,
                previewingPath: previewingPath,
                libraryStatus: nil,
                hasCoverOptions: true,
                coverContent: nil,
                editor: editor,
                storageManaged: storageManaged,
                storagePinned: storagePinned,
                mappingActions: inertMappingActions(),
                commitActions: ImportCommitActions(
                    confirmImport: {},
                    viewInLibrary: { _ in },
                ),
                onSetIdentity: { _ in },
                onFindRelease: {},
                onPickRelease: { _ in },
                onToggleSignal: { _ in },
                onEditCover: {},
                onSetClaimLevel: { _ in },
            )
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
            chooseFile: { _, _ in },
            drop: { _ in },
            exclude: { _ in },
        )
    }

    #Preview("Mapping pane — a release picked") {
        @Previewable
        @State
        var values = PreviewData.confirmEditValues
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.mappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned,
            previewingPath: "/tmp/fake/Track 3.flac"
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — nothing picked yet") {
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unidentifiedMappingCandidate,
            editor: nil,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 700)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — several matches to pick") {
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.severalMatchesMappingCandidate,
            editor: nil,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — a cue carving one container") {
        @Previewable
        @State
        var values = PreviewData.confirmEditValues
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.sheetMappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
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
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
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
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.moreTracksMappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
        .frame(width: 760, height: 900)
        .importPreviewEnvironment()
    }

    #Preview("Mapping pane — read as Unknown") {
        @Previewable
        @State
        var values = PreviewData.confirmEditValues
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unknownMappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }
#endif
