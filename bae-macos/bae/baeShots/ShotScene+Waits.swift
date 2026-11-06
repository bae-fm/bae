import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Views in a wait that names itself in words: a catalog still
    /// searching, a cast connecting, a Discogs key being checked, the
    /// lightbox's image loading, the run's own pick being read, an import
    /// waiting its turn.
    static let waitScenes: [ShotScene] = [
        ShotScene(
            id: "import-search-searching",
            size: CGSize(width: 800, height: 520)
        ) {
            let images = PreviewData.artImageStore()
            return ShotStage(
                FindOnlineSearchResults(
                    search: PreviewData.searchRunInFlight,
                    isImporting: false,
                    libraryStatuses: [:],
                    selectedReleaseId: nil,
                    loadingPick: nil,
                    onRetry: {},
                    onSelect: { _ in },
                )
                .importPreviewEnvironment(images: images),
                images: images
            )
        },
        ShotScene(
            id: "cast-picker-connecting",
            size: CGSize(width: 300, height: 160)
        ) {
            ShotStage(
                CastPickerPopover(
                    devices: [
                        BridgeCastDevice(
                            id: "speaker-1",
                            name: "Kitchen",
                            kind: .cast
                        )
                    ],
                    castingDevice: nil,
                    endedFailure: nil,
                    connectingName: "Kitchen",
                    onCast: { _ in },
                    onDisconnect: {}
                )
                .padding()
            )
        },
        ShotScene(
            id: "settings-discogs-checking",
            size: CGSize(width: 500, height: 320)
        ) {
            ShotStage(
                DiscogsKeyPreview(
                    status: .unvalidated,
                    draft: "abcdef123456",
                    isValidating: true
                )
            )
        },
        ShotScene(
            id: "artwork-lightbox-loading",
            size: CGSize(width: 640, height: 480)
        ) {
            ShotStage(LightboxView<LightboxItem>.loadingPreview())
        },
        ShotScene(
            id: "import-search-reading-pick",
            size: CGSize(width: 620, height: 120)
        ) {
            let pressing = PreviewData.searchGroupExact.pressings[0]
            let images = PreviewData.artImageStore()
            return ShotStage(
                ImportSearchResultRow(
                    pressing: pressing,
                    isImporting: false,
                    libraryStatus: nil,
                    isSelected: true,
                    loading: LoadingPick(
                        releaseId: pressing.lead.releaseId,
                        reading: .unattended
                    ),
                    onSelect: { _ in }
                )
                .frame(width: 620, height: 120, alignment: .top)
                .importPreviewEnvironment(images: images),
                images: images
            )
        },
        ShotScene(
            id: "import-pane-queued",
            size: CGSize(width: 560, height: 400)
        ) {
            let images = ImageStore.stub()
            return ShotStage(
                ImportingCandidatePane(
                    candidate: PreviewData.folderCandidates[0],
                    step: .preparing(step: .queued),
                    coverContent: nil,
                    onOpenImages: { _, _ in },
                    onOpenDocument: { _, _ in },
                    onPreview: { _ in },
                    onStopPreview: {},
                    previewingTarget: nil
                )
                .candidateReaderPreviewEnvironment()
                .environment(images),
                images: images
            )
        },
    ]
}
