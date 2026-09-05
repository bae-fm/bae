import BaeKit
import SwiftUI

/// Candidate artwork uses scanned-file choices; persisted releases use the
/// same gallery with library file identities through CoverSheetView.
struct CoverPickerView: View {
    let remoteCoverArts: [BridgeRemoteCover]
    let localArtwork: [BridgeCandidateFile]
    let selectedCover: BridgeCoverChoice?
    let onSelect: (BridgeCoverChoice) -> Void
    let onDone: () -> Void

    var body: some View {
        CoverGalleryView(
            remoteItems: remoteCoverArts.map {
                CoverItem(coverChoice: $0.coverChoice, label: $0.label)
            },
            releaseItems: localArtwork.map { file in
                guard let choice = file.coverChoice else {
                    preconditionFailure(
                        "The artwork picker received a non-image file"
                    )
                }
                return CoverItem(coverChoice: choice, label: file.file.name)
            },
            selectedCover: selectedCover?.selection,
            onSelect: { item in
                guard case .candidate(let choice) = item.content else {
                    preconditionFailure(
                        "A candidate picker cannot contain library files"
                    )
                }
                onSelect(choice)
            },
            onDone: onDone
        )
    }
}

#if DEBUG
    #Preview("Cover picker") {
        CoverPickerView(
            remoteCoverArts: PreviewData.remoteCovers,
            localArtwork: PreviewData.bridgeCandidateFiles.images,
            selectedCover: PreviewData.remoteCovers.first?.coverChoice,
            onSelect: { _ in },
            onDone: {}
        )
        .frame(width: 1_000, height: 740)
        .importPreviewEnvironment()
    }
#endif
