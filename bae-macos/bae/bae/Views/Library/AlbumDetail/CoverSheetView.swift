import BaeKit
import SwiftUI

/// A library release's cover picker: the covers core reads for it under the
/// source chosen, with a lookup of the catalogs' covers that core stores.
/// Scanned candidate paths never enter this picker.
struct CoverSheetView: View {
    let releaseId: String
    /// The lightbox's image as it loads, owned by whoever shows the sheet.
    let lightboxLoad: LightboxImageLoad
    let initialLayout: ArtworkBrowserState.Layout
    let onFindRelease: (() -> Void)?
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void

    @State
    private var state = CoverPickerState()
    @State
    private var covers: LibraryCoverState
    @Environment(Library.self)
    private var library

    init(
        releaseId: String,
        lightboxLoad: LightboxImageLoad,
        initialLayout: ArtworkBrowserState.Layout = .grid,
        onFindRelease: (() -> Void)? = nil,
        refreshCovers: @escaping () async throws -> Void,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void
    ) {
        self.releaseId = releaseId
        self.lightboxLoad = lightboxLoad
        self.initialLayout = initialLayout
        self.onFindRelease = onFindRelease
        self.onSelect = onSelect
        self.onDone = onDone
        _covers = State(
            initialValue: LibraryCoverState(refresh: refreshCovers)
        )
    }

    var body: some View {
        @Bindable
        var covers = covers
        CoverGalleryView(
            content: covers.content,
            remoteItems: covers.lookup,
            filter: $covers.filter,
            selectedCover: nil,
            lightboxLoad: lightboxLoad,
            initialLayout: initialLayout,
            isSaving: state.isSaving,
            error: state.error ?? covers.readFailure,
            onRefresh: { covers.startRefresh() },
            onFindRelease: onFindRelease,
            onSelect: { item in
                guard let selection = item.selection else {
                    preconditionFailure(
                        "The current cover cannot be applied as a new selection"
                    )
                }
                state.save(
                    { try await onSelect(selection) },
                    onSaved: onDone
                )
            },
            onDone: onDone
        )
        .task(id: releaseId) {
            await covers.open(
                releaseId: releaseId,
                query: library.coverGallery()
            )
        }
        .onDisappear {
            state.cancel()
            covers.cancel()
        }
    }
}
