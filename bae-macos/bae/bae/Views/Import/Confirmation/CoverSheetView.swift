import BaeKit
import SwiftUI

/// Shows the covers that core reads for a library release's or a
/// candidate's picker under the source the person picks, and the catalog
/// lookup whose covers core stores.
struct CoverSheetView: View {
    /// The cover the owner has now, which the sheet can't pick again; `nil`
    /// where the gallery shows the saved cover itself.
    let selectedCover: BridgeCoverSelection?
    /// The lightbox's image as it loads, owned by whoever shows the sheet.
    let lightboxLoad: LightboxImageLoad
    let onFindRelease: (() -> Void)?
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void
    /// Whether the sheet opens on the lightbox at its focused cover.
    private let opensOnLightbox: Bool

    @State
    private var state = CoverPickerState()
    @State
    private var covers: CoverGalleryState
    @Environment(Importer.self)
    private var importer

    init(
        owner: BridgeCoverGalleryOwner,
        opensOn focus: BridgeCoverKey? = nil,
        selectedCover: BridgeCoverSelection? = nil,
        lightboxLoad: LightboxImageLoad,
        onFindRelease: (() -> Void)? = nil,
        refreshCovers: @escaping () async throws -> Void,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void
    ) {
        self.selectedCover = selectedCover
        self.lightboxLoad = lightboxLoad
        self.onFindRelease = onFindRelease
        self.onSelect = onSelect
        self.onDone = onDone
        opensOnLightbox = focus != nil
        let covers = CoverGalleryState(owner: owner, refresh: refreshCovers)
        covers.focusedCover = focus
        _covers = State(initialValue: covers)
    }

    var body: some View {
        @Bindable
        var covers = covers
        MacCoverGallery(
            content: covers.content,
            remoteItems: covers.lookup,
            filter: $covers.filter,
            focusedCover: $covers.focusedCover,
            selectedCover: selectedCover,
            lightboxLoad: lightboxLoad,
            opensOnLightbox: opensOnLightbox,
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
            await covers.open(query: importer.coverGallery())
        }
        .onDisappear {
            state.cancel()
            covers.cancel()
        }
    }
}
