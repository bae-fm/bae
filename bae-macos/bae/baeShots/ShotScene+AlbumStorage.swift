import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    static let albumStorageScenes: [ShotScene] = [
        ShotScene(
            id: "mac.album.storage",
            size: albumStorageSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumStorageStage(.cloud)
        },
        ShotScene(
            id: "mac.album.storage.uploading",
            size: albumStorageSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumStorageStage(.uploading)
        },
        ShotScene(
            id: "mac.album.storage.unpin",
            size: albumStorageSize,
            frame: .window(title: "bae", titleBar: .hidden)
        ) {
            albumStorageStage(.askingToUnpin)
        },
    ]

    private enum AlbumStorageStage {
        case cloud
        case uploading
        case askingToUnpin
    }

    private static let albumStorageSize = CGSize(width: 1_200, height: 1_020)

    private static func albumStorageStage(_ stage: AlbumStorageStage)
        -> ShotStage
    {
        let uiStore = UiStore()
        let detail = albumStorageDetail(stage)
        let release = detail.shownRelease
        let backing = boardLibraryBacking(
            uiStore: uiStore,
            albumCount: 4,
            openAlbum: true,
            detail: detail,
            releaseFiles: albumStorageFiles
        )
        let title = detail.album.title
        let sheetRelease = ReleaseDetail(bridge: release)
        uiStore.presentModal {
            ReleaseStorageSheet(
                release: sheetRelease,
                albumTitle: title,
                asksBeforeUnpinning: true,
                onAction: { _ in },
                onUnpinConfirmed: { _ in },
                onCancelPin: {},
                onExport: {},
                onSaveAs: {},
                onDone: {}
            )
            .frame(width: 640, height: 600)
            .background(Theme.surfaceElevated)
        }
        let images = coverGalleryImages()
        let window = ShotScenes.libraryWindow(
            uiStore: uiStore,
            backing: backing,
            playbackStore: boardPlayback(),
            images: images,
            outbox: stage == .uploading
                ? albumStorageOutbox(release) : nil,
            size: albumStorageSize
        )
        return ShotStage(
            Group {
                if stage == .askingToUnpin {
                    OverWindow.alert(
                        UnpinConfirmation.alert(
                            for: PendingUnpin(
                                releaseIds: [release.id],
                                title: title
                            ),
                            dontShowAgain: .constant(false)
                        ) { _ in },
                        over: window
                    )
                }
                else {
                    window
                }
            },
            images: images
        ) {
            backing.session.albums.list?.delivered != nil
        }
    }

    private static func albumStorageDetail(_ stage: AlbumStorageStage)
        -> BridgeAlbumDetail
    {
        var detail = boardAlbumDetail()
        var release = detail.shownRelease
        switch stage {
        case .cloud:
            release.storageState = .inStorage(pinned: false)
            release.storageActions = [.pin, .moveToThisDevice]
        case .uploading:
            release.upload = shotUpload(queued: 0, uploading: 1)
            release.transitioning = true
        case .askingToUnpin:
            release.storageState = .inStorage(pinned: true)
            release.storageActions = [.unpin, .moveToThisDevice]
        }
        release.fileCount = Int64(albumStorageFiles.count)
        release.totalSize = albumStorageFiles.reduce(0) { $0 + $1.fileSize }
        detail.releases = [release]
        detail.shownRelease = release
        return detail
    }

    private static func albumStorageOutbox(_ release: BridgeRelease)
        -> OutboxStore
    {
        OutboxStore(
            uploadsPaused: false,
            releaseProgress: .releaseUploads(
                holding: [
                    release.id: BridgeReleaseUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: release.totalSize * 2 / 5,
                            bytesTotal: release.totalSize,
                            fraction: 0.4
                        ),
                        throughputBps: 6_400_000
                    )
                ]
            )
        )
    }

    /// The Storage boards' files, in the initial sort by name.
    private static let albumStorageFiles: [BridgeFile] = {
        let audio = zip(
            PreviewData.lowTideHoursTracks,
            [UInt64(34_100_000), 38_900_000, 28_200_000]
        )
        .map { track, size in
            releaseFile(
                track.file,
                audio: PreviewData.previewReleaseAudioFormat,
                size: size,
                contentType: "audio/flac"
            )
        }
        var cover = PreviewData.previewReleaseCoverFile
        cover.fileSize = 1_200_000
        return audio + [
            cover,
            releaseFile(
                "Low Tide Hours.cue",
                size: 2_000,
                kind: .cueSheet,
                contentType: "application/x-cue"
            ),
            releaseFile(
                "Low Tide Hours.log",
                size: 6_000,
                kind: .ripLog,
                contentType: "text/plain"
            ),
        ]
    }()
}
