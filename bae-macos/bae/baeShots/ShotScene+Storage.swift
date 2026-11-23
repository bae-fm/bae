import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Storage pane's boards (`mac.settings.storage*`): what this Mac
    /// keeps, with data a reset left behind, and the alert that asks before
    /// deleting it.
    static let storageSettingsScenes: [ShotScene] = [
        ShotScene(
            id: "mac.settings.storage",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewUsual)
            return ShotStage(
                SettingsWindowShot(
                    tab: .storage,
                    page: storageSettingsPreview(store)
                )
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.from-before-reset",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                SettingsWindowShot(
                    tab: .storage,
                    page: storageSettingsPreview(store)
                )
            ) { store.storage != nil }
        },
        ShotScene(
            id: "mac.settings.storage.delete-from-before-reset",
            size: CGSize(width: 690, height: 600),
            frame: .window(title: "Settings", titleBar: .hidden)
        ) {
            let store = DeviceStorageStore.staging(.previewBeforeReset)
            return ShotStage(
                OverWindow.alert(
                    StorageAlerts.deleteBeforeReset(
                        size: UInt64(960_000_000).fileSizeText
                    ),
                    over: SettingsWindowShot(
                        tab: .storage,
                        page: storageSettingsPreview(store)
                    )
                )
            ) { store.storage != nil }
        },
    ]
    /// The view of a Storage Manager board's window, which the board draws
    /// 1,100 by 660 points with its 52-point title bar and toolbar inside;
    /// the shot's window adds that title bar to the view's size.
    static let storageManagerSize = CGSize(width: 1_100, height: 608)

    /// The Storage Manager window with no library open and with a release
    /// selected as its files wait or download into the chosen folder.
    static let storageManagerScenes: [ShotScene] = [
        ShotScene(
            id: "mac.storage.window.no-library",
            size: storageManagerSize,
            frame: .window(title: "Storage Manager")
        ) {
            ShotStage(StorageManagerNoLibrary())
        },
        ShotScene(
            id: "mac.storage.window.move-to-this-device-queued",
            size: storageManagerSize,
            frame: .window(title: "Storage Manager")
        ) {
            moveToThisMac(moving: false)
        },
        ShotScene(
            id: "mac.storage.window.moving-to-mac",
            size: storageManagerSize,
            frame: .window(title: "Storage Manager")
        ) {
            moveToThisMac(moving: true)
        },
    ]

    private static let moveFiles = [
        releaseFile(
            "01 Harbor Lights.flac",
            size: 34_100_000,
            kind: .audio(format: "FLAC")
        ),
        releaseFile(
            "02 The Long Way Home.flac",
            size: 38_900_000,
            kind: .audio(format: "FLAC")
        ),
        releaseFile(
            "03 Second Floor.flac",
            size: 28_200_000,
            kind: .audio(format: "FLAC")
        ),
        releaseFile(
            "04 Low Tide Hours.flac",
            size: 37_600_000,
            kind: .audio(format: "FLAC")
        ),
        releaseFile("cover.jpg", size: 1_200_000, kind: .image(format: "JPEG")),
        releaseFile("Glass Harbor.cue", size: 2_000, kind: .cueSheet),
    ]

    private static func moveToThisMac(moving: Bool) -> ShotStage {
        let rows = moveRows(moving: moving)
        let move = stagedDownload(
            rows[4],
            kind: .moveToThisDevice,
            active: moving
        )
        let pin = stagedDownload(rows[5], kind: .pin, active: !moving)
        let currentFile = moveFiles[3]
        let answers = UnansweredCount()
        let fixture = StorageManagerPreviewFixture(
            rows: rows,
            selectedReleaseId: move.releaseId,
            inspectorPresented: true,
            downloads: [move, pin],
            outputs: [],
            uploads: [],
            answers: answers,
            queues: boardStorageQueues(
                pinning: moving ? move.releaseId : pin.releaseId
            ),
            inspectorOverrides: [
                move.releaseId: moveInspector(
                    move,
                    current: moving ? currentFile : nil
                )
            ]
        )
        let streams = BoardTransferStreams(pinning: pin.releaseId)
        return ShotStage(
            fixture.environment(
                of: MacStorageWindow(
                    initialSelection: fixture.initialSelection,
                    initialInspectorPresented: true
                ),
                downloads: moving
                    ? moveStreams(move, currentFile: currentFile)
                    : streams.downloads,
                outputs: streams.outputs,
                outbox: streams.outbox
            )
        ) {
            fixture.storageManagerStore.list?.count != nil && answers.value == 0
        }
    }

    private static func moveRows(moving: Bool) -> [BridgeStorageRow] {
        var rows = boardStorageRows
        rows[4].status = .movingToThisDevice(step: moving ? .running : .queued)
        rows[4].release.transitioning = true
        rows[4].release.fileCount = Int64(moveFiles.count)
        rows[4].release.totalSize = moveFiles.reduce(UInt64(0)) {
            $0 + $1.fileSize
        }
        rows[4].cancelOnly = true
        if moving {
            // A pin shares the same queue, so it waits while the move runs.
            rows[5].status = .pinning(step: .queued)
        }
        return rows
    }

    private static func stagedDownload(
        _ row: BridgeStorageRow,
        kind: BridgeDownloadKind,
        active: Bool
    ) -> BridgeDownloadOp {
        BridgeDownloadOp(
            releaseId: row.id,
            kind: kind,
            fileCount: UInt64(row.release.fileCount),
            totalSize: row.release.totalSize,
            release: BridgeQueuedRelease(title: row.album.title),
            createdAt: PreviewData.queuedAt(minutesAgo: 1),
            state: active ? .active : .queued
        )
    }

    private static func moveInspector(
        _ move: BridgeDownloadOp,
        current: BridgeFile?
    ) -> BridgeStorageInspector {
        BridgeStorageInspector(
            transfers: [.download(operation: move, summary: nil)],
            files: moveFiles.enumerated()
                .map { index, file in
                    let download: BridgeFileDownload
                    if current != nil && index < 3 {
                        download = .done
                    }
                    else if file.id == current?.id {
                        download = .downloading(
                            kind: .moveToThisDevice,
                            fileId: file.id
                        )
                    }
                    else {
                        download = .waiting(kind: .moveToThisDevice)
                    }
                    return BridgeStorageInspectorFile(
                        file: file,
                        upload: nil,
                        download: download,
                        problem: nil
                    )
                }
        )
    }

    private static func moveStreams(
        _ move: BridgeDownloadOp,
        currentFile: BridgeFile
    ) -> DownloadStore {
        DownloadStore(
            moveFileShares: .fileShares(holding: [currentFile.id: 0.4]),
            moveToThisDeviceTransfers: .movesToThisDevice(
                holding: [
                    move.releaseId: BridgeMoveToThisDeviceProgress(
                        fileBytesDone: 15_040_000,
                        fileBytesTotal: currentFile.fileSize,
                        bytesDone: 116_240_000,
                        bytesTotal: move.totalSize,
                        fraction: Double(116_240_000) / Double(move.totalSize)
                    )
                ]
            )
        )
    }
}
