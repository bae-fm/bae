import SwiftUI

/// The file pane + right pane layout used when a candidate is selected.
/// FolderImportTab uses this for its main content area.
struct ImportMainPane<RightPane: View>: View {
    let files: CandidateFiles
    let onOpenGallery: (Int) -> Void
    let onOpenDocument: (String, String) -> Void
    let onPreviewAudio: (String) -> Void
    /// Surface errors from file operations (e.g. readTextFile).
    let onError: (String) -> Void
    let previewState: PreviewState
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
            )
            .frame(minWidth: 200, idealWidth: 280, maxWidth: 400)
            rightPane()
                .frame(minWidth: 300, maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

// MARK: - Previews

#Preview("Auto-lookup in progress") {
    ImportMainPane(
        files: PreviewData.candidateFiles,
        onOpenGallery: { _ in },
        onOpenDocument: { _, _ in },
        onPreviewAudio: { _ in },
        onError: { _ in },
        previewState: .idle,
    ) {
        ImportSearchPane(
            identifyState: .triangulating(
                discid: .lookingUp,
                barcode: .skipped,
            ),
            showManualSearch: false,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            activeTab: .constant(.general),
            activeSource: .constant(.musicBrainz),
            searchArtist: .constant(""),
            searchAlbum: .constant(""),
            searchCatalog: .constant(""),
            searchBarcode: .constant(""),
            discogsEnabled: false,
            signals: nil,
            signalsToolbar: SignalsToolbar(signals: [
                ToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .lookingUp,
                    excluded: false
                ),
                ToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: nil,
                    origin: .artwork,
                    state: .skipped,
                    excluded: false
                ),
            ]),
            onSearch: {},
            onOpenSettings: {},
            onSearchManually: {},
            onViewMatches: {},
            onAddAsUnknown: {},
            onToggleSignal: { _ in },
            onRerun: {},
            onSelect: { _ in },
        )
    }
    .frame(width: 900, height: 600)
    .environment(MediaPaths.stub)
    .environment(UiStore())
}

#Preview("Manual search — no results") {
    ImportMainPane(
        files: PreviewData.candidateFiles,
        onOpenGallery: { _ in },
        onOpenDocument: { _, _ in },
        onPreviewAudio: { _ in },
        onError: { _ in },
        previewState: .idle,
    ) {
        ImportSearchPane(
            identifyState: .notFoundAnywhere,
            showManualSearch: true,
            error: nil,
            searchGroups: [],
            selectedReleaseId: nil,
            isSearching: false,
            hasSearched: false,
            isImporting: false,
            libraryStatuses: [:],
            activeTab: .constant(.general),
            activeSource: .constant(.musicBrainz),
            searchArtist: .constant(""),
            searchAlbum: .constant(""),
            searchCatalog: .constant(""),
            searchBarcode: .constant(""),
            discogsEnabled: true,
            signals: nil,
            signalsToolbar: SignalsToolbar(signals: [
                ToolbarSignal(
                    kind: .discId,
                    role: .identity,
                    value: "disc-hash",
                    origin: .discToc,
                    state: .noMatch,
                    excluded: false
                ),
                ToolbarSignal(
                    kind: .barcode,
                    role: .identity,
                    value: "5051961234567",
                    origin: .artwork,
                    state: .noMatch,
                    excluded: false
                ),
            ]),
            onSearch: {},
            onOpenSettings: {},
            onSearchManually: {},
            onViewMatches: {},
            onAddAsUnknown: {},
            onToggleSignal: { _ in },
            onRerun: {},
            onSelect: { _ in },
        )
    }
    .frame(width: 900, height: 600)
    .environment(MediaPaths.stub)
    .environment(UiStore())
}
