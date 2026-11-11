import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// A release's records and storage: the records row naming every
    /// catalog, the card behind the facts line, and the Storage sheet over a
    /// pinned release.
    static let releaseScenes: [ShotScene] = [
        ShotScene(
            id: "release-records-every-catalog",
            size: CGSize(width: 420, height: 120)
        ) {
            ShotStage(
                ReleaseRecordsRow(
                    records: PreviewData.releaseRecordsEveryCatalog
                )
                .padding()
            )
        },
        ShotScene(
            id: "release-records-card",
            size: CGSize(width: ReleaseRecordsCard.width, height: 96)
        ) {
            ShotStage(
                ReleaseRecordsCard(
                    records: PreviewData.identifiedFromBothCatalogs
                )
            )
        },
        ShotScene(
            id: "release-storage-sheet",
            size: CGSize(width: 640, height: 600)
        ) {
            let release = PreviewData.storageBridgeRelease(
                storageState: .inStorage(pinned: true),
                storageActions: [.unpin, .detachAudio],
                playability: .playable
            )
            let answers = UnansweredCount()
            // The sheet asks for the release and its files as it appears,
            // a main-actor step after it is first drawn, which the tree's
            // coming to rest spans.
            return ShotStage(
                ReleaseStorageSheet(
                    release: ReleaseDetail(bridge: release),
                    onAction: { _ in },
                    onCancelPin: {},
                    onExport: {},
                    onSaveAs: {},
                    onDone: {}
                )
                .background(Theme.background)
                .environment(
                    Library(
                        releaseDetail: {
                            .fixed(answers: answers) { _ in release }
                        },
                        releaseFiles: {
                            .fixed(
                                PreviewData.previewReleaseFiles,
                                answers: answers
                            )
                        }
                    )
                )
                .environment(OutboxStore(uploadsPaused: false))
                .environment(DownloadStore())
            ) { answers.value == 0 }
        },
        // A release's Storage band in each state it can be in: one change on
        // its way as one pill, what needs the person as a notice, and the
        // actions that state offers.
        ShotScene(
            id: "release-storage-band-states",
            size: CGSize(width: 560, height: 1_160)
        ) {
            ShotStage(StorageBandStatesShot())
        },
    ]
}

/// One Storage band per state, stacked.
private struct StorageBandStatesShot: View {
    var body: some View {
        VStack(spacing: 0) {
            ForEach(Self.states, id: \.name) { state in
                StorageStatusBand(
                    release: state.release,
                    onAction: { _ in },
                    onCancelPin: {},
                    onExport: {},
                    onSaveAs: {}
                )
                Divider()
            }
        }
        .frame(width: 560)
        .background(Theme.background)
        .environment(OutboxStore(uploadsPaused: false))
        .environment(PreviewData.downloadStore())
    }

    private struct BandState {
        let name: String
        let release: ReleaseDetail
    }

    private static let states: [BandState] = [
        BandState(
            name: "queued pin",
            release: release(
                actions: [.pin, .detachAudio],
                downloadStatus: .queued
            )
        ),
        BandState(
            name: "pinning",
            release: release(actions: [], downloadStatus: .downloading)
        ),
        BandState(
            name: "failed pin",
            release: release(
                actions: [.pin, .detachAudio],
                downloadStatus: .failed(
                    error: "The network connection was lost."
                )
            )
        ),
        BandState(
            name: "queued upload",
            release: release(
                storageState: .onOneDevice,
                actions: [],
                upload: upload(.queued)
            )
        ),
        BandState(
            name: "uploading",
            release: release(
                storageState: .onOneDevice,
                actions: [],
                upload: upload(.uploading)
            )
        ),
        BandState(
            name: "missing original",
            release: release(
                storageState: .onOneDevice,
                actions: [],
                upload: upload(.queued, missing: "01 Track Title.flac")
            )
        ),
        BandState(
            name: "queued make local",
            release: release(actions: [], makeLocal: .queued)
        ),
        BandState(
            name: "making local",
            release: release(actions: [], makeLocal: .making)
        ),
        BandState(
            name: "failed make local",
            release: release(
                actions: [.pin, .detachAudio],
                makeLocal: .failed(error: "The disk is full.")
            )
        ),
    ]

    private static func upload(
        _ activity: BridgeUploadActivity,
        missing: String? = nil
    ) -> BridgeReleaseUpload {
        BridgeReleaseUpload(
            progress: BridgeUploadProgress(
                queued: 1,
                uploading: activity == .uploading ? 1 : 0,
                activity: activity
            ),
            summary: [],
            missing: missing
        )
    }

    private static func release(
        storageState: BridgeReleaseStorageState = .inStorage(pinned: false),
        actions: [BridgeReleaseStorageAction],
        upload: BridgeReleaseUpload? = nil,
        downloadStatus: BridgeReleaseDownloadStatus? = nil,
        makeLocal: BridgeMakeLocalStatus? = nil
    ) -> ReleaseDetail {
        var bridge = PreviewData.storageBridgeRelease(
            storageState: storageState,
            storageActions: actions,
            playability: .playable
        )
        bridge.upload = upload
        bridge.transitioning =
            upload != nil || makeLocal == .queued || makeLocal == .making
            || downloadStatus == .downloading
        bridge.downloadStatus = downloadStatus
        bridge.makeLocal = makeLocal
        return ReleaseDetail(bridge: bridge)
    }
}
