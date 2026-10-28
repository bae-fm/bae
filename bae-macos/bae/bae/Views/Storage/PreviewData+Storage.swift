#if DEBUG
    import BaeKit
    import SwiftUI

    // Preview fixtures for the Storage Manager: its transfer/sync queue rows
    // (downloads, exports, cloud-outbox uploads) and its release /
    // file table cells, plus a seeded `Library` + stores so the whole screen
    // renders offline. Generic placeholder names only.
    @MainActor
    extension PreviewData {
        // MARK: - Enqueue times

        /// An enqueue time `minutes` in the past, as Unix epoch milliseconds —
        /// what the queue rows render as a "2m ago" relative label.
        static func queuedAt(minutesAgo minutes: Int) -> Int64 {
            Int64(Date().timeIntervalSince1970 * 1000)
                - Int64(minutes) * 60_000
        }

        // MARK: - Download (pin) queue

        /// The bytes the previews' downloads have moved: the download
        /// queue's active entry, and the storage rows' release pinning.
        static let downloadTransfers: BridgeDownloadTransferChange = .reset(
            transfers: [
                BridgeKeyedDownloadTransfer(
                    releaseId: "rel-row-1",
                    progress: BridgeDownloadTransferProgress(
                        bytesDone: 140_000_000,
                        bytesTotal: 367_000_000,
                        fraction: 140.0 / 367.0,
                        throughputBps: 12_100_000
                    )
                ),
                BridgeKeyedDownloadTransfer(
                    releaseId: "rel-row-5",
                    progress: BridgeDownloadTransferProgress(
                        bytesDone: 151_000_000,
                        bytesTotal: 463_000_000,
                        fraction: 151.0 / 463.0,
                        throughputBps: 12_100_000
                    )
                ),
            ]
        )

        /// One active, one queued, one failed — the three download-row states.
        static let downloadOps: [BridgeDownloadOp] = [
            BridgeDownloadOp(
                releaseId: "rel-row-1",
                release: BridgeQueuedRelease(
                    title:
                        "Album Title with an Intentionally Long Descriptive Subtitle",
                    fileCount: 12,
                    totalSize: 367_000_000
                ),
                createdAt: queuedAt(minutesAgo: 1),
                state: .active
            ),
            BridgeDownloadOp(
                releaseId: "rel-row-2",
                release: BridgeQueuedRelease(
                    title: "B",
                    fileCount: 9,
                    totalSize: 210_000_000
                ),
                createdAt: queuedAt(minutesAgo: 3),
                state: .queued
            ),
            BridgeDownloadOp(
                releaseId: "rel-row-3",
                release: BridgeQueuedRelease(
                    title: "Album Title C",
                    fileCount: 15,
                    totalSize: 512_000_000
                ),
                createdAt: queuedAt(minutesAgo: 8),
                state: .failed(error: "The network connection was lost.")
            ),
        ]

        /// The storage rows' release pinning (`rel-row-5`), downloading.
        static var pinningDownloads: [BridgeDownloadOp] {
            [
                BridgeDownloadOp(
                    releaseId: "rel-row-5",
                    release: nil,
                    createdAt: queuedAt(minutesAgo: 1),
                    state: .active
                )
            ]
        }

        /// The storage rows' release pinning (`rel-row-5`), waiting its
        /// turn in the download queue.
        static var queuedPinDownloads: [BridgeDownloadOp] {
            [
                BridgeDownloadOp(
                    releaseId: "rel-row-5",
                    release: nil,
                    createdAt: queuedAt(minutesAgo: 1),
                    state: .queued
                )
            ]
        }

        /// How far the previews' files being downloaded to be kept have
        /// got: the pinning release's audio, as its inspector shows it.
        static let pinFileTransfers: BridgeFileTransferChange = .reset(
            files: [
                BridgeKeyedFileTransfer(
                    fileId: pinningFileId("rel-row-5-audio"),
                    fraction: 0.42
                )
            ]
        )

        /// The file coven downloads for a storage row release's file.
        static func pinningFileId(_ releaseFileId: String) -> String {
            "pin-\(releaseFileId)"
        }

        /// The previews' download bytes, leaving out every release
        /// `downloads` holds waiting or failed: one not running has moved
        /// nothing.
        @MainActor
        static func downloadStore(
            downloads: [BridgeDownloadOp] = downloadOps
        ) -> DownloadStore {
            let resting = Set(
                downloads.filter {
                    if case .active = $0.state {
                        false
                    }
                    else {
                        true
                    }
                }
                .map(\.releaseId)
            )
            let store = DownloadStore()
            guard case .reset(let transfers) = downloadTransfers,
                case .reset(let files) = pinFileTransfers
            else { preconditionFailure("the preview transfers are resets") }
            store.applyTransferChange(
                .reset(
                    transfers: transfers.filter {
                        !resting.contains($0.releaseId)
                    }
                )
            )
            store.applyFileTransferChange(
                .reset(
                    files: files.filter { file in
                        !resting.contains {
                            file.fileId.hasPrefix(pinningFileId($0))
                        }
                    }
                )
            )
            return store
        }

        // MARK: - Export / save queue

        /// One active save, one queued export, one failed export — the output-row
        /// states across both kinds.
        static let outputOps: [BridgeOutputOp] = [
            BridgeOutputOp(
                releaseId: "rel-row-1",
                targetDir: "/Music/Exports",
                release: BridgeQueuedRelease(
                    title: "Album Title A",
                    fileCount: 12,
                    totalSize: 213_000_000
                ),
                createdAt: queuedAt(minutesAgo: 1),
                state: .active,
                kind: .save(presetName: "FLAC")
            ),
            BridgeOutputOp(
                releaseId: "rel-row-2",
                targetDir: "/Music/Exports",
                release: BridgeQueuedRelease(
                    title: "Album Title B",
                    fileCount: 9,
                    totalSize: 340_000_000
                ),
                createdAt: queuedAt(minutesAgo: 4),
                state: .queued,
                kind: .export
            ),
            BridgeOutputOp(
                releaseId: "rel-row-3",
                targetDir: "/Music/Exports",
                release: BridgeQueuedRelease(
                    title: "Album Title C",
                    fileCount: 15,
                    totalSize: 512_000_000
                ),
                createdAt: queuedAt(minutesAgo: 9),
                state: .failed(error: "Destination folder is not writable."),
                kind: .export
            ),
        ]

        /// The percent the preview's active save has reached.
        static let outputTransfers: BridgeOutputTransferChange = .reset(
            transfers: [
                BridgeKeyedOutputTransfer(releaseId: "rel-row-1", percent: 62)
            ]
        )

        @MainActor
        static func outputStore() -> OutputStore {
            let store = OutputStore()
            store.applyTransferChange(outputTransfers)
            return store
        }

        // MARK: - Cloud outbox

        /// A file coven can't upload until its source is back where it was.
        static let unavailableFilePending = BridgePendingWait(
            reason: BridgePendingReason(
                key: "core.pending.reason.file_unavailable",
                detail:
                    "file 0b6f2c9e-5a1d-4c7e-9f3a-2d8e6b1c4a70 is missing at /Volumes/Music/Album Title/05 Track Title.flac",
                causes: []
            ),
            action: .restoreFile
        )

        /// A release with files in the previews' upload queue: what its row
        /// reads of its upload, and each of its files' uploads by file id.
        struct PreviewReleaseUpload {
            let releaseId: String
            let upload: BridgeReleaseUpload
            let files: [String: BridgeFileUpload]
        }

        /// One count of a transfer summary, as core words it.
        static func transferCount(
            _ key: String,
            _ count: UInt32,
            _ stage: BridgeTransferStage
        ) -> BridgeTransferCount {
            BridgeTransferCount(
                label: BridgeCountLabel(key: key, count: count),
                stage: stage
            )
        }

        /// A release whose audio is being sent and whose cover waits its
        /// turn. How far the audio has got is the preview's upload
        /// transfers.
        static let uploadingRelease = PreviewReleaseUpload(
            releaseId: "rel-row-1",
            upload: BridgeReleaseUpload(
                progress: BridgeUploadProgress(
                    queued: 1,
                    uploading: 1,
                    activity: .uploading
                ),
                summary: [
                    transferCount("core.queue.uploading", 1, .moving),
                    transferCount("core.queue.queued", 1, .waiting),
                ]
            ),
            files: [
                "rel-row-1-audio": BridgeFileUpload(
                    fileId: "upload-rel-row-1-audio",
                    state: .uploading,
                    pending: nil
                ),
                "rel-row-1-image": BridgeFileUpload(
                    fileId: "upload-rel-row-1-image",
                    state: .waiting,
                    pending: nil
                ),
            ]
        )

        /// A release whose one file waits its turn.
        static let queuedRelease = PreviewReleaseUpload(
            releaseId: "rel-row-2",
            upload: BridgeReleaseUpload(
                progress: BridgeUploadProgress(
                    queued: 1,
                    uploading: 0,
                    activity: .queued
                ),
                summary: [transferCount("core.queue.queued", 1, .waiting)]
            ),
            files: [
                "rel-row-2-audio": BridgeFileUpload(
                    fileId: "upload-rel-row-2-audio",
                    state: .waiting,
                    pending: nil
                )
            ]
        )

        /// A release whose only file waits on its missing source.
        static let pendingRelease = PreviewReleaseUpload(
            releaseId: "rel-row-3",
            upload: BridgeReleaseUpload(
                progress: BridgeUploadProgress(
                    queued: 1,
                    uploading: 0,
                    activity: .queued
                ),
                summary: [transferCount("core.queue.queued", 1, .waiting)]
            ),
            files: [
                "rel-row-3-audio": BridgeFileUpload(
                    fileId: "upload-rel-row-3-audio",
                    state: .waiting,
                    pending: unavailableFilePending
                )
            ]
        )

        static let defaultUploads = [uploadingRelease, queuedRelease]

        /// `rows`, each release's row reading its upload among `uploads`.
        static func storageRows(
            _ rows: [BridgeStorageRow],
            uploading uploads: [PreviewReleaseUpload]
        ) -> [BridgeStorageRow] {
            rows.map { row in
                guard
                    let upload = uploads.first(where: {
                        $0.releaseId == row.id
                    })
                else { return row }
                var row = row
                row.release.upload = upload.upload
                row.release.transitioning = true
                row.status = .uploading
                return row
            }
        }

        /// How far and how fast the preview's uploading release goes: of its
        /// 36.4 MB, the 12.4 MB of its audio sent; and the bytes the files
        /// being sent have moved together.
        static let uploadTransfers: BridgeOutboxTransferChange = .reset(
            releases: [
                BridgeKeyedReleaseUploadTransfer(
                    releaseId: "rel-row-1",
                    transfer: BridgeReleaseUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 12_400_000,
                            bytesTotal: 36_400_000
                        ),
                        throughputBps: 3_200_000
                    )
                )
            ],
            total: BridgeUploadTransfer(
                bar: BridgeUploadBar(
                    bytesDone: 22_400_000,
                    bytesTotal: 57_100_000
                ),
                throughputBps: 6_800_000
            )
        )

        /// How far the preview's file being sent has got: 12.4 MB of its
        /// 34 MB.
        static let uploadFileTransfers: BridgeFileTransferChange = .reset(
            files: [
                BridgeKeyedFileTransfer(
                    fileId: "upload-rel-row-1-audio",
                    fraction: 12_400_000 / 34_000_000
                )
            ]
        )

        @MainActor
        static func outboxStore() -> OutboxStore {
            let store = OutboxStore(uploadsPaused: false)
            store.applyTransferChange(uploadTransfers)
            store.applyFileTransferChange(uploadFileTransfers)
            return store
        }

        // MARK: - Storage table rows

        /// A release summary in a chosen storage state, built through the wire
        /// type so `ReleaseSummary`'s real projection runs. `transfer` seeds an
        /// in-flight transition badge.
        @MainActor
        static func storageRelease(
            id: String = "rel-store-1",
            albumId: String = "album-store-1",
            media: [BridgeMediaCount] = PreviewData.media(.cd),
            storageState: BridgeReleaseStorageState = .remote,
            pinned: Bool = false,
            transfer: BridgeReleaseStorageAction? = nil,
            upload: BridgeReleaseUpload? = nil,
            fileCount: Int64 = 12,
            totalSize: Int64 = 367_000_000
        ) -> ReleaseSummary {
            ReleaseSummary(
                from: BridgeReleaseSummary(
                    id: id,
                    albumId: albumId,
                    media: media,
                    mediaTerms: bridgeMediaTerms(media: media),
                    storageState: storageState,
                    pinned: pinned,
                    storageActions: [],
                    transferAction: transfer,
                    upload: upload,
                    transitioning: upload != nil || transfer != nil,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    cover: nil
                )
            )
        }

        static let storageAlbum = BridgeAlbum(
            id: "album-store-1",
            title: "Album Title",
            year: 2021,
            isCompilation: false,
            artistNames: "Artist Name",
            releaseIds: ["rel-store-1"],
            primaryReleaseId: "rel-store-1",
            cover: nil
        )

        // MARK: - Whole-screen: seeded list + Library

        /// Dense rows spanning the names, media, storage states, file counts,
        /// and sizes the whole-screen previews must keep readable.
        static let storageRows: [BridgeStorageRow] = {
            let titles = [
                "Album Title with an Intentionally Long Descriptive Subtitle",
                "B",
                "Album Title C",
                "Two-Disc Archival Collection with Additional Session Material",
                "Live Set",
                "Untitled Recording",
                "Album Title — Expanded Edition",
                "Collection Volume 08",
            ]
            let artists = [
                "Artist Name with Multiple Collaborators and Ensemble Members",
                "A",
                "Artist Name C",
                "Various Artists",
                "Ensemble Name",
                "Unknown Artist",
            ]
            let media: [[BridgeMediaCount]] = [
                PreviewData.media(.cd), PreviewData.media(.vinyl, 2),
                PreviewData.media(.cassette), PreviewData.media(.miniDisc),
                PreviewData.media(.digital), [],
            ]

            return (1...28)
                .map { index in
                    let isRemote = index % 3 != 0
                    let pinned = isRemote && index % 4 == 0
                    let pinning = index == 5
                    return storageRow(
                        index: index,
                        title: titles[(index - 1) % titles.count],
                        artist: artists[(index - 1) % artists.count],
                        year: index % 7 == 0 ? nil : Int32(1980 + index),
                        media: media[(index - 1) % media.count],
                        storageState: isRemote ? .remote : .local,
                        pinned: pinned,
                        transfer: pinning ? .pin : nil,
                        status: pinning
                            ? .pinning
                            : isRemote ? (pinned ? .pinned : .cloud) : .local,
                        fileCount: Int64(1 + index % 24),
                        totalSize: Int64(48_000_000 + index * 83_000_000)
                    )
                }
        }()

        /// The storage row of release `rel-row-<index>`, on album
        /// `album-row-<index>`.
        private static func storageRow(
            index: Int,
            title: String,
            artist: String,
            year: Int32? = 2021,
            media: [BridgeMediaCount] = PreviewData.media(.cd),
            storageState: BridgeReleaseStorageState,
            pinned: Bool = false,
            transfer: BridgeReleaseStorageAction? = nil,
            status: BridgeStorageRowStatus,
            upload: BridgeReleaseUpload? = nil,
            fileCount: Int64 = 12,
            totalSize: Int64 = 210_000_000
        ) -> BridgeStorageRow {
            let releaseId = "rel-row-\(index)"
            let albumId = "album-row-\(index)"
            return BridgeStorageRow(
                release: BridgeReleaseSummary(
                    id: releaseId,
                    albumId: albumId,
                    media: media,
                    mediaTerms: bridgeMediaTerms(media: media),
                    storageState: storageState,
                    pinned: pinned,
                    storageActions: [],
                    transferAction: transfer,
                    upload: upload,
                    transitioning: upload != nil || transfer != nil,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    cover: nil
                ),
                album: BridgeAlbum(
                    id: albumId,
                    title: title,
                    year: year,
                    isCompilation: false,
                    artistNames: artist,
                    releaseIds: [releaseId],
                    primaryReleaseId: releaseId,
                    cover: nil
                ),
                status: status
            )
        }

        /// A pre-populated `StorageList` for the footer and whole-screen
        /// previews — seeded with the rows so the table paints without an
        /// async round-trip.
        @MainActor
        static func storageList(
            rows: [BridgeStorageRow] = storageRows
        ) -> StorageList {
            let list = StorageList(
                pageSource: storageLibrary(rows: rows).storagePageSource,
                onError: { _ in }
            )
            list.preloadForPreview(
                rows: storageRows(rows, uploading: defaultUploads)
            )
            return list
        }

        /// A `Library` whose storage reads serve the fixture rows and nothing
        /// else — enough for `StorageManagerView`'s own `rebuildList()` to
        /// populate the table — and whose inspector shows each row's release
        /// with its entry in `downloads`, `outputs` and `uploads`.
        static func storageLibrary(
            rows: [BridgeStorageRow] = storageRows,
            downloads: [BridgeDownloadOp] = downloadOps,
            outputs: [BridgeOutputOp] = outputOps,
            uploads: [PreviewReleaseUpload] = defaultUploads
        ) -> Library {
            let rows = storageRows(rows, uploading: uploads)
            let details = Dictionary(
                uniqueKeysWithValues: rows.map { row in
                    (row.id, storageReleaseDetail(for: row))
                }
            )
            let inspectors = details.mapValues { detail in
                storageInspector(
                    release: detail,
                    download: downloads.first { $0.releaseId == detail.id },
                    output: outputs.first { $0.releaseId == detail.id },
                    upload: uploads.first { $0.releaseId == detail.id }
                )
            }
            let queues = storageQueues(
                downloads: downloads,
                outputs: outputs,
                uploads: uploads
            )
            return Library(
                releaseDetail: { .fixed { details[$0] } },
                storageBrowse: { _, _ in .fixed(rows, queues: queues) },
                storageInspector: { .fixed { inspectors[$0] } }
            )
        }

        /// The queues holding the fixture's entries, as core's list value
        /// names them: each running, the serial ones at their active entry.
        private static func storageQueues(
            downloads: [BridgeDownloadOp],
            outputs: [BridgeOutputOp],
            uploads: [PreviewReleaseUpload]
        ) -> BridgeStorageQueues {
            BridgeStorageQueues(
                uploads: uploads.isEmpty
                    ? nil : BridgeUploadQueueRun(paused: false),
                pins: downloads.isEmpty
                    ? nil
                    : BridgeReleaseQueueRun(
                        paused: false,
                        active: downloads.first {
                            if case .active = $0.state {
                                true
                            }
                            else {
                                false
                            }
                        }?
                        .releaseId
                    ),
                outputs: outputs.isEmpty
                    ? nil
                    : BridgeReleaseQueueRun(
                        paused: false,
                        active: outputs.first {
                            if case .active = $0.state {
                                true
                            }
                            else {
                                false
                            }
                        }?
                        .releaseId
                    )
            )
        }

        /// What core's inspector read delivers for `release` beside its
        /// queue entries. While its download waits or runs, its images are
        /// kept, and its audio downloads while the download runs and waits
        /// while it waits its turn, as core counts them.
        private static func storageInspector(
            release: BridgeRelease,
            download: BridgeDownloadOp?,
            output: BridgeOutputOp?,
            upload: PreviewReleaseUpload?
        ) -> BridgeStorageInspector {
            let pinning: Bool
            let downloading: Bool
            switch download?.state {
            case .active: (pinning, downloading) = (true, true)
            case .queued: (pinning, downloading) = (true, false)
            case .failed, nil: (pinning, downloading) = (false, false)
            }
            let pin = { (file: BridgeFile) -> BridgeFilePin? in
                guard pinning else { return nil }
                if file.isImage { return .kept }
                return downloading
                    ? .downloading(fileId: pinningFileId(file.id)) : .waiting
            }
            var transfers: [BridgeStorageInspectorTransfer] = []
            if let download {
                // Core leaves out a zero count; the fixture's release has
                // one image kept and one audio file downloading or waiting.
                transfers.append(
                    .download(
                        operation: download,
                        summary: pinning
                            ? [
                                downloading
                                    ? transferCount(
                                        "core.queue.pinning",
                                        1,
                                        .moving
                                    )
                                    : transferCount(
                                        "core.queue.waiting",
                                        1,
                                        .waiting
                                    ),
                                transferCount("core.queue.pinned", 1, .done),
                            ] : nil
                    )
                )
            }
            if let output {
                transfers.append(.output(operation: output))
            }
            if let upload {
                transfers.append(
                    .upload(
                        releaseId: upload.releaseId,
                        upload: upload.upload
                    )
                )
            }
            return BridgeStorageInspector(
                transfers: transfers,
                files: release.files.map {
                    BridgeStorageInspectorFile(
                        file: $0,
                        upload: upload?.files[$0.id],
                        pin: pin($0)
                    )
                }
            )
        }

        /// The format of every storage row release's audio file.
        private static let storageAudioFormat = BridgeAudioFormat(
            codec: "FLAC",
            sampleRateHz: 44_100,
            bitsPerSample: 16,
            bitrateKbps: nil,
            channels: 2
        )

        /// A storage row release's audio file and its cover.
        private static func storageReleaseFiles(
            for row: BridgeStorageRow
        ) -> (audio: BridgeFile, image: BridgeFile) {
            (
                BridgeFile(
                    id: "\(row.id)-audio",
                    originalFilename: "01 Track Title.flac",
                    name: "01 Track Title.flac",
                    folder: nil,
                    kind: .audio(format: "FLAC"),
                    fileSize: 34_000_000,
                    contentType: "audio/flac",
                    isImage: false,
                    audioFormat: storageAudioFormat
                ),
                BridgeFile(
                    id: "\(row.id)-image",
                    originalFilename: "cover.jpg",
                    name: "cover.jpg",
                    folder: nil,
                    kind: .image(format: "JPEG"),
                    fileSize: 2_400_000,
                    contentType: "image/jpeg",
                    isImage: true,
                    audioFormat: nil
                )
            )
        }

        private static func storageReleaseDetail(
            for row: BridgeStorageRow
        ) -> BridgeRelease {
            let audioFormat = storageAudioFormat
            let (audioFile, imageFile) = storageReleaseFiles(for: row)
            let files = [audioFile, imageFile]
            let facts = PreviewData.pressingFacts(media: row.release.media)
            return BridgeRelease(
                id: row.release.id,
                albumId: row.release.albumId,
                name: .named(name: row.album.title),
                year: row.album.year,
                labels: [],
                facts: facts,
                pressingSummary: bridgePressingSummary(facts: facts),
                mediaTerms: bridgeMediaTerms(media: facts.media),
                pressingDetails: bridgePressingDetails(facts: facts),
                storageState: row.release.storageState,
                pinned: row.release.pinned,
                storageActions: row.release.storageActions,
                transferAction: row.release.transferAction,
                upload: row.release.upload,
                transitioning: row.release.transitioning,
                tracks: [],
                trackGroups: [],
                files: files,
                sourceAudio: .uniform(
                    descriptor: BridgeSourceAudioDescriptor(
                        layout: .file,
                        format: audioFormat
                    )
                ),
                imageFiles: [imageFile],
                coverFiles: [imageFile],
                galleryItems: [],
                playability: .playable,
                downloadStatus: nil,
                records: [],
                totalDuration: nil,
                fileCount: Int64(files.count),
                totalSize: files.reduce(Int64(0)) { $0 + $1.fileSize },
                cover: row.release.cover
            )
        }
    }

    extension Library {
        /// The storage page source over this library — the seam
        /// `StorageManagerView` builds internally, exposed for the preview list.
        fileprivate var storagePageSource: StorageBrowsePageSource {
            StorageBrowsePageSource(
                library: self,
                sort: BridgeStorageSort(
                    field: .albumTitle,
                    direction: .ascending
                ),
                filter: .all,
                selected: [],
                onListValue: { _ in }
            )
        }
    }
#endif
