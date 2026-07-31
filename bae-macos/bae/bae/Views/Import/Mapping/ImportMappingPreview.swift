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
                .environment(ImageStore.stub)
                .environment(UiStore())
                .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
                .environment(PreviewData.configStore)
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
                model: ImportMappingModel(
                    files: candidate.files,
                    slots: candidate.slots,
                    edit: editor?.wrappedValue
                ),
                bindingOptions: PreviewData.sheetBindingOptions,
                previewingPath: previewingPath,
                libraryStatus: nil,
                hasCoverOptions: true,
                coverContent: nil,
                editor: editor,
                storageManaged: storageManaged,
                storagePinned: storagePinned,
                roleActions: ImportRoleActions(
                    setRole: { _, _ in },
                    bindSheet: { _, _ in },
                    openDocument: { _ in },
                    openImage: { _ in },
                ),
                slotActions: ImportSlotActions(
                    preview: { _ in },
                    stopPreview: {},
                    chooseFile: { _, _ in },
                    drop: { _ in },
                    exclude: { _ in },
                ),
                commitActions: ImportCommitActions(
                    confirmImport: {},
                    viewInLibrary: { _ in },
                ),
                onFindRelease: {},
                onEditCover: {},
            )
        }
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

    #Preview("Mapping pane — a file the release doesn't name") {
        @Previewable
        @State
        var values = PreviewData.unmatchedEditValues
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMappingPreview.make(
            candidate: PreviewData.unmatchedMappingCandidate,
            editor: $values,
            storageManaged: $storageManaged,
            storagePinned: $storagePinned
        )
        .frame(width: 1212, height: 900)
        .importPreviewEnvironment()
    }
#endif
