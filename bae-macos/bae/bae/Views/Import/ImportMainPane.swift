import BaeKit
import SwiftUI

/// The file pane + right pane layout used when a candidate is selected.
/// ImportView uses this for its main content area.
struct ImportMainPane<RightPane: View>: View {
    let files: BridgeCandidateFiles
    let onOpenGallery: (Int) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreviewAudio: (String) -> Void
    /// Surface errors from file operations (e.g. readTextFile).
    let onError: (String) -> Void
    let previewState: PreviewState
    /// What each track sheet may be bound to — see `ImportFilePane`.
    let bindingOptions: [String: [BridgeSheetBindingOption]]
    let onBind: (String, String?) -> Void
    @ViewBuilder
    let rightPane: () -> RightPane

    var body: some View {
        HSplitView {
            ImportFilePane(
                files: files,
                onOpenGallery: onOpenGallery,
                onOpenDocument: onOpenDocument,
                onPreviewAudio: onPreviewAudio,
                onError: onError,
                previewState: previewState,
                bindingOptions: bindingOptions,
                onBind: onBind,
            )
            .frame(minWidth: 200, idealWidth: 280, maxWidth: 400)
            rightPane()
                .frame(minWidth: 300, maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Auto-lookup in progress") {
        ImportMainPane(
            files: PreviewData.bridgeCandidateFiles,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .idle,
            bindingOptions: PreviewData.sheetBindingOptions,
            onBind: { _, _ in },
        ) {
            ImportResultPane(open: false, onClose: {}) {
                ImportSearchPane.preview(
                    state: PreviewData.searchStateTriangulating
                )
            } pane: {
                EmptyView()
            }
        }
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }

    #Preview("Manual search — no results") {
        ImportMainPane(
            files: PreviewData.bridgeCandidateFiles,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .idle,
            bindingOptions: PreviewData.sheetBindingOptions,
            onBind: { _, _ in },
        ) {
            ImportResultPane(open: false, onClose: {}) {
                ImportSearchPane.preview(state: PreviewData.searchStateNotFound)
            } pane: {
                EmptyView()
            }
        }
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }

    #Preview("Main pane — tracks, manual search") {
        ImportMainPane(
            files: PreviewData.candidateFilesTracks,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .idle,
            bindingOptions: PreviewData.sheetBindingOptions,
            onBind: { _, _ in },
        ) {
            ImportResultPane(open: false, onClose: {}) {
                ImportSearchPane.preview(state: PreviewData.searchStateNotFound)
            } pane: {
                EmptyView()
            }
        }
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }

    #Preview("Main pane — confirming (tracks)") {
        @Previewable
        @State
        var values = PreviewData.confirmEditValues
        @Previewable
        @State
        var storageManaged = true
        @Previewable
        @State
        var storagePinned = true
        ImportMainPane(
            files: PreviewData.candidateFilesTracks,
            onOpenGallery: { _ in },
            onOpenDocument: { _, _ in },
            onPreviewAudio: { _ in },
            onError: { _ in },
            previewState: .idle,
            bindingOptions: PreviewData.sheetBindingOptions,
            onBind: { _, _ in },
        ) {
            ImportResultPane(open: true, onClose: {}) {
                ImportSearchPane.preview(
                    state: PreviewData.searchStateFoundExact
                )
            } pane: {
                ImportConfirmationPreview.make(
                    values: $values,
                    storageManaged: $storageManaged,
                    storagePinned: $storagePinned
                )
            }
        }
        .frame(width: 1212, height: 982)
        .importPreviewEnvironment()
    }
#endif
