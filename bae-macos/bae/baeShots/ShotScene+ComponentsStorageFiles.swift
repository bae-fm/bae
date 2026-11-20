import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// The Mac components of the storage inspector and a release's files:
    /// the failed transfer's row, a transfer's readout, a file's row and
    /// details, a queue's rate, and the files of a release.
    static let storageFileComponentScenes: [ShotScene] =
        failedTransferRowScenes + transferReadoutScenes + fileRowScenes
        + fileDetailsScenes + queueRateScenes + releaseFileRowScenes
        + releaseFileTableScenes

    // MARK: Failed transfer row

    private static func failedDownload(
        _ kind: BridgeDownloadKind
    ) -> BridgeDownloadOp {
        BridgeDownloadOp(
            releaseId: "rel-glass-harbor",
            kind: kind,
            fileCount: 9,
            totalSize: 356_000_000,
            release: BridgeQueuedRelease(title: "Glass Harbor"),
            createdAt: 0,
            state: .failed(error: "The network connection was lost.")
        )
    }

    private static func failedTransferRow(
        _ id: String,
        entry: MacFailedTransferRow.Entry,
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: 456,
            height: height
        ) {
            MacFailedTransferRow(entry: entry, onCancel: {})
        }
    }

    /// `MacFailedTransferRow`: a pin and a move to This Mac that failed, an
    /// upload held by a missing original, and a move to the cloud that
    /// failed because an original changed.
    private static let failedTransferRowScenes: [ShotScene] = [
        failedTransferRow(
            "mac.component.failed-transfer-row.pin",
            entry: .download(failedDownload(.pin), onRetry: {}),
            height: 130
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.move-to-this-mac",
            entry: .download(failedDownload(.moveToThisDevice), onRetry: {}),
            height: 130
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.upload-waiting",
            entry: .uploadWaiting(
                title: "Glass Harbor",
                fileCount: 9,
                totalSize: 356_000_000,
                missing: "01 Harbor Lights.flac"
            ),
            height: 150
        ),
        failedTransferRow(
            "mac.component.failed-transfer-row.move-to-the-cloud-failed",
            entry: .moveFailed(
                title: "Glass Harbor",
                fileCount: 9,
                totalSize: 356_000_000,
                changed: "01 Harbor Lights.flac",
                reason: DisplayError(
                    line: "Could not move to the cloud",
                    detail: "The original changed after the move began."
                )
            ),
            height: 150
        ),
    ]

    // MARK: Transfer readout

    /// `MacTransferReadout`, its bytes and rate and its bar read from the
    /// transfer's stream by native views, as the app draws them.
    private static let transferReadoutScenes: [ShotScene] = [
        component("mac.component.transfer-readout.upload", height: 60) {
            let outbox = OutboxStore(
                uploadsPaused: false,
                releaseProgress: .releaseUploads(
                    holding: [
                        "rel-readout": BridgeReleaseUploadTransfer(
                            bar: BridgeUploadBar(
                                bytesDone: 140_000_000,
                                bytesTotal: 356_000_000,
                                fraction: 0.4
                            ),
                            throughputBps: 6_400_000
                        )
                    ]
                )
            )
            MacTransferReadout(
                stream: .upload(
                    outbox.uploadProgress(forRelease: "rel-readout")
                ),
                kind: .upload
            )
            .frame(width: 380)
            .environment(outbox)
        },
        component("mac.component.transfer-readout.pin", height: 60) {
            let downloads = DownloadStore(
                releaseProgress: .downloads(
                    holding: [
                        "rel-readout": BridgeDownloadTransferProgress(
                            bytesDone: 12_000_000,
                            bytesTotal: 344_000_000,
                            fraction: 0.1,
                            throughputBps: 3_100_000
                        )
                    ]
                )
            )
            MacTransferReadout(
                stream: .pin(
                    downloads.downloadProgress(forRelease: "rel-readout")
                ),
                kind: .pin
            )
            .frame(width: 380)
            .environment(downloads)
        },
    ]

    // MARK: File row and details

    /// A file of a release, named `name`: audio when it has an `audio`
    /// format, else the `kind` it names.
    private static func releaseFile(
        _ name: String,
        audio: BridgeAudioFormat? = nil,
        size: UInt64,
        kind: BridgeFileKind? = nil,
        contentType: String = "application/octet-stream"
    ) -> BridgeFile {
        BridgeFile(
            id: name,
            originalFilename: name,
            name: name,
            folder: nil,
            kind: kind ?? (audio == nil ? .other : .audio(format: "FLAC")),
            fileSize: size,
            contentType: contentType,
            isImage: false,
            audioFormat: audio
        )
    }

    private static func inspectorFile(
        _ name: String,
        size: UInt64,
        kind: BridgeFileKind,
        upload: BridgeFileUpload? = nil,
        pin: BridgeFilePin? = nil,
        problem: BridgeFileProblem? = nil
    ) -> BridgeStorageInspectorFile {
        BridgeStorageInspectorFile(
            file: releaseFile(name, size: size, kind: kind),
            upload: upload,
            pin: pin,
            problem: problem
        )
    }

    private static func fileRow(
        _ id: String,
        _ row: BridgeStorageInspectorFile,
        selected: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            Group {
                if selected {
                    AccentSelectionList(
                        rows: [row],
                        id: \.file.id,
                        selection: Binding<String?>.constant(row.file.id)
                    ) { row in
                        MacFileRow(row: row)
                    }
                    .frame(height: 36)
                }
                else {
                    MacFileRow(row: row)
                }
            }
            .frame(width: 380, alignment: .leading)
            .environment(
                OutboxStore(
                    uploadsPaused: false,
                    fileShares: .fileShares(
                        holding: ["01 Harbor Lights.flac": 0.7]
                    )
                )
            )
            .environment(
                DownloadStore(
                    fileShares: .fileShares(
                        holding: ["03 Second Floor.flac": 0.4]
                    )
                )
            )
        }
    }

    /// `MacFileRow`, a file of the inspector at rest, waiting, moving,
    /// selected, the file a move stopped on because its original changed,
    /// and the file an upload waits for because its original is missing.
    private static let fileRowScenes: [ShotScene] = [
        fileRow(
            "mac.component.file-row.at-rest",
            inspectorFile(
                "cover.jpg",
                size: 1_200_000,
                kind: .image(format: "JPEG")
            )
        ),
        fileRow(
            "mac.component.file-row.queued",
            inspectorFile(
                "02 The Long Way Home.flac",
                size: 38_900_000,
                kind: .audio(format: "FLAC"),
                upload: BridgeFileUpload(
                    fileId: "02 The Long Way Home.flac",
                    state: .waiting,
                    pending: nil
                )
            )
        ),
        fileRow(
            "mac.component.file-row.sending",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                upload: BridgeFileUpload(
                    fileId: "01 Harbor Lights.flac",
                    state: .uploading,
                    pending: nil
                )
            )
        ),
        fileRow(
            "mac.component.file-row.pinning",
            inspectorFile(
                "03 Second Floor.flac",
                size: 28_200_000,
                kind: .audio(format: "FLAC"),
                pin: .downloading(fileId: "03 Second Floor.flac")
            )
        ),
        fileRow(
            "mac.component.file-row.error",
            inspectorFile(
                "01 Harbor Lights.flac",
                size: 34_100_000,
                kind: .audio(format: "FLAC"),
                problem: .originalChanged
            )
        ),
        fileRow(
            "mac.component.file-row.waiting",
            inspectorFile(
                "01 Evening Lines.flac",
                size: 31_000_000,
                kind: .audio(format: "FLAC"),
                problem: .originalMissing
            )
        ),
        fileRow(
            "mac.component.file-row.selected",
            inspectorFile(
                "Low Tide Hours.cue",
                size: 2_000,
                kind: .cueSheet
            ),
            selected: true
        ),
        fileRow(
            "mac.component.file-row.long-name",
            inspectorFile(
                "01 Harbor Lights (Recorded Live at the Harbor Hall, Second Night).flac",
                size: 98_400_000,
                kind: .audio(format: "FLAC")
            )
        ),
    ]

    /// `MacFileDetails`, an audio file's with its folder and format and a
    /// log's with neither. The audio file's kind reads "FLAC audio": the
    /// component says "Audio".
    private static let fileDetailsScenes: [ShotScene] = [
        component("mac.component.file-details.audio-file", height: 200) {
            MacFileDetails(
                file: BridgeFile(
                    id: "file-flac",
                    originalFilename: "01 Harbor Lights.flac",
                    name: "01 Harbor Lights.flac",
                    folder: "Mara Quill/Low Tide Hours",
                    kind: .audio(format: "FLAC"),
                    fileSize: 34_100_000,
                    contentType: "audio/flac",
                    isImage: false,
                    audioFormat: BridgeAudioFormat(
                        codec: "FLAC",
                        sampleRateHz: 44_100,
                        bitsPerSample: 16,
                        bitrateKbps: nil,
                        channels: 2
                    )
                ),
                onClose: {}
            )
            .frame(width: 380, alignment: .leading)
        },
        component("mac.component.file-details.other-file", height: 160) {
            MacFileDetails(
                file: BridgeFile(
                    id: "file-log",
                    originalFilename: "Field Recordings.log",
                    name: "Field Recordings.log",
                    folder: nil,
                    kind: .ripLog,
                    fileSize: 6_000,
                    contentType: "text/plain",
                    isImage: false,
                    audioFormat: nil
                ),
                onClose: {}
            )
            .frame(width: 380, alignment: .leading)
        },
    ]

    // MARK: Queue rate

    /// One transfer queue's rate or hold, over the streams the app's footer
    /// reads: uploads 6.4 MB/s, pins 3.1 MB/s, exports 18 percent.
    private static func queueRate(
        _ id: String,
        _ queue: StorageQueueKind,
        hold: String? = nil,
        paused: Bool = false
    ) -> ShotScene {
        component(id, height: 50) {
            let outbox = OutboxStore(
                uploadsPaused: false,
                filesBeingSent: .uploadTotal(
                    holding: BridgeUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 140_000_000,
                            bytesTotal: 356_000_000,
                            fraction: 0.4
                        ),
                        throughputBps: 6_400_000
                    )
                )
            )
            let downloads = DownloadStore(
                releaseProgress: .downloads(
                    holding: [
                        "rel-rate": BridgeDownloadTransferProgress(
                            bytesDone: 12_000_000,
                            bytesTotal: 344_000_000,
                            fraction: 0.1,
                            throughputBps: 3_100_000
                        )
                    ]
                )
            )
            let outputs = OutputStore(
                percents: .exportPercents(holding: ["rel-rate": 18])
            )
            MacQueueRate(queue: queue, paused: paused, hold: hold) {
                switch queue {
                case .uploads:
                    StreamLabel(stream: .uploadRate(outbox.totalProgress))
                case .downloads:
                    StreamLabel(
                        stream: .pinRate(
                            downloads.downloadProgress(forRelease: "rel-rate")
                        )
                    )
                case .outputs:
                    StreamLabel(
                        stream: .exportPercent(
                            outputs.exportProgress(forRelease: "rel-rate")
                        )
                    )
                }
            }
            .environment(outbox)
            .environment(downloads)
            .environment(outputs)
            .environment(Sync.stub())
            .environment(Downloads.stub())
            .environment(Outputs.stub())
            .environment(UiStore())
        }
    }

    /// 6:40 PM today, as the upload queue's retry time.
    private static let retryTimeMs: Int64 = {
        let time =
            Calendar.current.date(
                bySettingHour: 18,
                minute: 40,
                second: 0,
                of: Date()
            ) ?? Date()
        return Int64(time.timeIntervalSince1970 * 1000)
    }()

    /// `MacQueueRate`, a queue's rate and a queue held back. An export has
    /// no rate in the app: its queue counts the percent written, where the
    /// component says "18 MB/s".
    private static let queueRateScenes: [ShotScene] = [
        queueRate("mac.component.queue-rate.uploads", .uploads),
        queueRate("mac.component.queue-rate.pins", .downloads),
        queueRate("mac.component.queue-rate.exports", .outputs),
        queueRate(
            "mac.component.queue-rate.waiting",
            .uploads,
            hold: BridgeUploadQueueWait.offline.text
        ),
        queueRate(
            "mac.component.queue-rate.retrying",
            .uploads,
            hold: BridgeUploadQueueWait.retrying(atMs: retryTimeMs).text
        ),
        queueRate(
            "mac.component.queue-rate.paused",
            .downloads,
            hold: BridgeQueueWait.paused.text,
            paused: true
        ),
    ]

    // MARK: Files of a release

    /// `MacReleaseFileRow`, a file of a release at the width the preview
    /// gives it: a document that opens, a file that opens nothing, a damaged
    /// one, an audio file with its Play button, and a long name.
    private static func releaseFileRow(
        _ id: String,
        name: String,
        size: String,
        damage: String? = nil,
        opens: Bool = false,
        plays: Bool = false
    ) -> ShotScene {
        component(
            id,
            width: 496,
            height: 40
        ) {
            MacReleaseFileRow(
                name: name,
                size: size,
                damage: damage,
                onOpen: opens ? {} : nil,
                onPlay: plays ? {} : nil
            )
            .frame(width: 480, alignment: .leading)
        }
    }

    private static let releaseFileRowScenes: [ShotScene] = [
        releaseFileRow(
            "mac.component.release-file-row.opens",
            name: "rip.log",
            size: "4 KB",
            opens: true
        ),
        releaseFileRow(
            "mac.component.release-file-row.other",
            name: ".DS_Store",
            size: "6 KB"
        ),
        releaseFileRow(
            "mac.component.release-file-row.damaged",
            name: "02 Margins.flac",
            size: "31.8 MB",
            damage: "Cannot decode"
        ),
        releaseFileRow(
            "mac.component.release-file-row.audio",
            name: "01 Northbound.flac",
            size: "33 MB",
            plays: true
        ),
        releaseFileRow(
            "mac.component.release-file-row.long-name",
            name:
                "Mara Quill - Low Tide Hours - 1992 - EAC rip log with the drive offset and every read.log",
            size: "4 KB",
            opens: true
        ),
    ]

    private static let byName = KeyPathComparator(
        \BridgeFile.originalFilename,
        comparator: .localizedStandard
    )

    /// `files` in the order core returns them for `sort`.
    private static func releaseFileTable(
        _ id: String,
        _ files: [BridgeFile],
        sort: KeyPathComparator<BridgeFile> = byName,
        height: CGFloat
    ) -> ShotScene {
        component(
            id,
            width: 560,
            height: height
        ) {
            MacReleaseFileTable(
                files: files,
                sortOrder: .constant([sort])
            )
            .frame(width: 544, alignment: .leading)
        }
    }

    private static let flac1644 = BridgeAudioFormat(
        codec: "FLAC",
        sampleRateHz: 44_100,
        bitsPerSample: 16,
        bitrateKbps: nil,
        channels: 2
    )

    private static let harborLightsFile = releaseFile(
        "01 Harbor Lights.flac",
        audio: flac1644,
        size: 34_100_000,
        contentType: "audio/flac"
    )
    private static let longWayHomeFile = releaseFile(
        "02 The Long Way Home.flac",
        audio: flac1644,
        size: 38_900_000,
        contentType: "audio/flac"
    )
    private static let coverFile = releaseFile(
        "cover.jpg",
        size: 1_200_000,
        contentType: "image/jpeg"
    )
    private static let cueFile = releaseFile(
        "Low Tide Hours.cue",
        size: 2_000,
        contentType: "application/x-cue"
    )
    private static let logFile = releaseFile(
        "Low Tide Hours.log",
        size: 6_000,
        contentType: "text/plain"
    )

    /// `MacReleaseFileTable`, a release's files as a table of name, audio
    /// format, size and kind, sorted by name and by size, largest first. The
    /// component draws a Files heading over an AppKit table; the app draws
    /// the heading in the Storage sheet, over a SwiftUI table, whose sortable
    /// columns the component's table does not have.
    private static let releaseFileTableScenes: [ShotScene] = [
        releaseFileTable(
            "mac.component.release-file-table.files",
            [harborLightsFile, longWayHomeFile, coverFile, cueFile, logFile],
            height: 190
        ),
        releaseFileTable(
            "mac.component.release-file-table.sorted-by-size",
            [longWayHomeFile, harborLightsFile, coverFile, logFile, cueFile],
            sort: KeyPathComparator(\BridgeFile.fileSize, order: .reverse),
            height: 190
        ),
        releaseFileTable(
            "mac.component.release-file-table.long-name",
            [
                releaseFile(
                    "01 Harbor Lights (Recorded Live at the Harbor Hall, Second Night).flac",
                    audio: BridgeAudioFormat(
                        codec: "FLAC",
                        sampleRateHz: 96_000,
                        bitsPerSample: 24,
                        bitrateKbps: nil,
                        channels: 2
                    ),
                    size: 98_400_000,
                    contentType: "audio/flac"
                )
            ],
            height: 90
        ),
    ]
}
