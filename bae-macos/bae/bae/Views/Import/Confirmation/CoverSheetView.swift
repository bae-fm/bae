import BaeKit
import SwiftUI

/// Shows the covers that core reads for a library release's or a
/// candidate's picker under the source the person picks, and the catalog
/// lookup whose covers core stores.
struct CoverSheetView: View {
    let owner: BridgeCoverGalleryOwner
    /// The cover the owner has now, which the sheet can't pick again; `nil`
    /// where the gallery shows the saved cover itself.
    let selectedCover: BridgeCoverSelection?
    /// The lightbox's image as it loads, owned by whoever shows the sheet.
    let lightboxLoad: LightboxImageLoad
    let onFindRelease: (() -> Void)?
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void

    @State
    private var state = CoverPickerState()
    @State
    private var covers: CoverGalleryState
    @Environment(Library.self)
    private var library

    init(
        owner: BridgeCoverGalleryOwner,
        selectedCover: BridgeCoverSelection? = nil,
        lightboxLoad: LightboxImageLoad,
        onFindRelease: (() -> Void)? = nil,
        refreshCovers: @escaping () async throws -> Void,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void
    ) {
        self.owner = owner
        self.selectedCover = selectedCover
        self.lightboxLoad = lightboxLoad
        self.onFindRelease = onFindRelease
        self.onSelect = onSelect
        self.onDone = onDone
        _covers = State(
            initialValue: CoverGalleryState(
                owner: owner,
                refresh: refreshCovers
            )
        )
    }

    var body: some View {
        @Bindable
        var covers = covers
        CoverGalleryView(
            content: covers.content,
            remoteItems: covers.lookup,
            filter: $covers.filter,
            focusedCover: $covers.focusedCover,
            selectedCover: selectedCover,
            lightboxLoad: lightboxLoad,
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
        .task {
            await covers.open(query: library.coverGallery())
        }
        .onDisappear {
            state.cancel()
            covers.cancel()
        }
    }
}
