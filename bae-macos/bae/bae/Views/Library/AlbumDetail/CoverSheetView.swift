import BaeKit
import SwiftUI

/// Persisted artwork is read by library file identity, including cloud-only
/// files. Scanned candidate paths never enter this picker.
struct CoverSheetView: View {
    let releaseId: String
    let initialLayout: ArtworkBrowserState.Layout
    let onFindRelease: (() -> Void)?
    let fetchRemoteCovers: () async throws -> BridgeRemoteCoverGallery
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void

    @Environment(Library.self)
    private var library
    @State
    private var state = CoverPickerState()
    @State
    private var release: ReleaseDetail?
    @State
    private var releaseError: String?

    init(
        releaseId: String,
        initialRelease: ReleaseDetail? = nil,
        initialLayout: ArtworkBrowserState.Layout = .grid,
        onFindRelease: (() -> Void)? = nil,
        fetchRemoteCovers:
            @escaping () async throws -> BridgeRemoteCoverGallery,
        onSelect: @escaping (BridgeCoverSelection) async throws -> Void,
        onDone: @escaping () -> Void
    ) {
        self.releaseId = releaseId
        self.initialLayout = initialLayout
        self.onFindRelease = onFindRelease
        self.fetchRemoteCovers = fetchRemoteCovers
        self.onSelect = onSelect
        self.onDone = onDone
        _release = State(initialValue: initialRelease)
    }

    var body: some View {
        CoverGalleryView(
            remoteItems: state.remoteItems,
            releaseItems: release?.imageFiles
                .map {
                    CoverItem(releaseId: releaseId, file: $0)
                } ?? [],
            selectedCover: nil,
            currentCover: release?.summary.cover
                .map {
                    CoverItem(releaseId: releaseId, cover: $0)
                },
            initialLayout: initialLayout,
            isSaving: state.isSaving,
            errorMessage: state.errorMessage ?? releaseError,
            onRefresh: { state.refresh(fetchRemoteCovers) },
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
        .task(id: releaseId) { await state.load(fetchRemoteCovers) }
        .task(id: releaseId) {
            for await result in library.releaseDetails(releaseId) {
                do {
                    guard let release = try result.get() else {
                        self.release = nil
                        releaseError = String(
                            localized:
                                "This release is no longer in the library."
                        )
                        continue
                    }
                    self.release = ReleaseDetail(
                        summary: ReleaseSummary(from: release),
                        bridge: release
                    )
                    releaseError = nil
                }
                catch { releaseError = error.displayLine }
            }
        }
        .onDisappear { state.cancel() }
    }
}
