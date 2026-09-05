import BaeKit
import SwiftUI

/// Candidate artwork uses scanned-file choices; persisted releases use the
/// same gallery with library file identities through CoverSheetView.
struct CoverPickerView: View {
    let localArtwork: [BridgeCandidateFile]
    let selectedCover: BridgeCoverChoice?
    let fetchRemoteCovers: () async throws -> BridgeRemoteCoverGallery
    let onFindRelease: () -> Void
    let onSelect: (BridgeCoverChoice) async throws -> Void
    let onDone: () -> Void
    @State
    private var state: CoverPickerState

    init(
        remoteCoverArts: [BridgeRemoteCover],
        localArtwork: [BridgeCandidateFile],
        selectedCover: BridgeCoverChoice?,
        fetchRemoteCovers:
            @escaping () async throws -> BridgeRemoteCoverGallery,
        onFindRelease: @escaping () -> Void,
        onSelect: @escaping (BridgeCoverChoice) async throws -> Void,
        onDone: @escaping () -> Void
    ) {
        self.localArtwork = localArtwork
        self.selectedCover = selectedCover
        self.fetchRemoteCovers = fetchRemoteCovers
        self.onFindRelease = onFindRelease
        self.onSelect = onSelect
        self.onDone = onDone
        _state = State(
            initialValue: CoverPickerState(initialCovers: remoteCoverArts)
        )
    }

    var body: some View {
        CoverGalleryView(
            remoteItems: state.remoteItems,
            releaseItems: localArtwork.map { file in
                guard let choice = file.coverChoice else {
                    preconditionFailure(
                        "The artwork picker received a non-image file"
                    )
                }
                return CoverItem(coverChoice: choice, label: file.file.name)
            },
            selectedCover: selectedCover?.selection,
            isSaving: state.isSaving,
            errorMessage: state.errorMessage,
            onRefresh: { state.refresh(fetchRemoteCovers) },
            onFindRelease: onFindRelease,
            onSelect: { item in
                guard case .candidate(let choice) = item.content else {
                    preconditionFailure(
                        "A candidate picker cannot contain library files"
                    )
                }
                state.save({ try await onSelect(choice) }, onSaved: onDone)
            },
            onDone: onDone
        )
        .task { await state.load(fetchRemoteCovers) }
        .onDisappear { state.cancel() }
    }
}

#if DEBUG
    #Preview("Cover picker") {
        CoverPickerView(
            remoteCoverArts: PreviewData.remoteCovers,
            localArtwork: PreviewData.bridgeCandidateFiles.images,
            selectedCover: PreviewData.remoteCovers.first?.coverChoice,
            fetchRemoteCovers: { .linked(covers: PreviewData.remoteCovers) },
            onFindRelease: {},
            onSelect: { _ in },
            onDone: {}
        )
        .frame(width: 1_000, height: 740)
        .importPreviewEnvironment()
    }
#endif
