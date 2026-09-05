import BaeKit
import SwiftUI

/// Persisted artwork is read by library file identity, including cloud-only
/// files. Scanned candidate paths never enter this picker.
struct CoverSheetView: View {
    let releaseId: String
    let fetchRemoteCovers: () async throws -> [BridgeRemoteCover]
    let onSelect: (BridgeCoverSelection) async throws -> Void
    let onDone: () -> Void

    @Environment(Library.self)
    private var library
    @State
    private var state = CoverPickerState()
    @State
    private var releaseFiles: [BridgeFile] = []
    @State
    private var releaseError: String?

    var body: some View {
        CoverGalleryView(
            remoteItems: (state.remoteCovers ?? [])
                .map {
                    CoverItem(coverChoice: $0.coverChoice, label: $0.label)
                },
            releaseItems: releaseFiles.map {
                CoverItem(releaseId: releaseId, file: $0)
            },
            selectedCover: nil,
            isLoading: state.isLoading,
            isSaving: state.isSaving,
            errorMessage: state.errorMessage ?? releaseError,
            onRefresh: { state.refresh(fetchRemoteCovers) },
            onSelect: { item in
                state.save(
                    { try await onSelect(item.selection) },
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
                        releaseFiles = []
                        releaseError = String(
                            localized:
                                "This release is no longer in the library."
                        )
                        continue
                    }
                    releaseFiles = release.imageFiles
                    releaseError = nil
                }
                catch { releaseError = error.displayLine }
            }
        }
        .onDisappear { state.cancel() }
    }
}
