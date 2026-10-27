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

        /// The bytes the preview's active download has moved.
        static let downloadTransfers: BridgeDownloadTransferChange = .reset(
            transfers: [
                BridgeKeyedDownloadTransfer(
                    releaseId: "rel-row-1",
                    progress: BridgeDownloadTransferProgress(
                        bytesDone: 140_000_000,
                        bytesTotal: 367_000_000,
                        fraction: 140.0 / 367.0
                    )
                )
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

        @MainActor
        static func downloadStore() -> DownloadStore {
            let store = DownloadStore()
            store.applyTransferChange(downloadTransfers)
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

        /// Waiting and uploading files, a bar on each one moving bytes, and one
        /// waiting on its missing source.
        static let uploadFileOps: [BridgeUploadFileOp] = [
            BridgeUploadFileOp(
                pending: nil,
                fileId: "covers:rel-row-1",
                label: .cover,
                state: .waiting
            ),
            BridgeUploadFileOp(
                pending: nil,
                fileId: "release_files:f-2",
                label: .filename(name: "02 Track Title.flac"),
                state: .uploading
            ),
            BridgeUploadFileOp(
                pending: nil,
                fileId: "release_files:f-3",
                label: .filename(name: "03 Track Title.flac"),
                state: .waiting
            ),
            BridgeUploadFileOp(
                pending: nil,
                fileId: "release_files:rel-row-1-audio",
                label: .filename(name: "04 Track Title.flac"),
                state: .uploading
            ),
            BridgeUploadFileOp(
                pending: unavailableFilePending,
                fileId: "release_files:f-5",
                label: .filename(name: "05 Track Title.flac"),
                state: .waiting
            ),
            BridgeUploadFileOp(
                pending: nil,
                fileId: "release_files:f-6",
                label: .filename(name: "06 Track Title.flac"),
                state: .waiting
            ),
        ]

        static func uploadProgress(
            activity: BridgeUploadActivity?
        ) -> BridgeUploadProgress {
            BridgeUploadProgress(
                queued: 4,
                uploading: 2,
                activity: activity
            )
        }

        static let uploadGroup = BridgeUploadReleaseGroup(
            releaseId: "rel-row-1",
            files: uploadFileOps,
            progress: uploadProgress(activity: .uploading)
        )

        /// A second group whose one file waits its turn.
        static let uploadGroupQueued = BridgeUploadReleaseGroup(
            releaseId: "rel-row-2",
            files: [
                BridgeUploadFileOp(
                    pending: nil,
                    fileId: "release_files:rel-row-2-audio",
                    label: .filename(name: "01 Track Title.flac"),
                    state: .waiting
                )
            ],
            progress: BridgeUploadProgress(
                queued: 1,
                uploading: 0,
                activity: .queued
            )
        )

        /// A group whose only file is waiting on its missing source.
        static let uploadGroupPending = BridgeUploadReleaseGroup(
            releaseId: "rel-row-3",
            files: [
                BridgeUploadFileOp(
                    pending: unavailableFilePending,
                    fileId: "release_files:rel-row-3-audio",
                    label: .filename(name: "01 Track Title.flac"),
                    state: .waiting
                )
            ],
            progress: BridgeUploadProgress(
                queued: 1,
                uploading: 0,
                activity: .queued
            )
        )

        static let defaultUploadGroups = [uploadGroup, uploadGroupQueued]

        /// What the rows of `defaultUploadGroups`' releases read of their
        /// uploads: two files being sent with their bytes, and one waiting.
        static let rowUploads: [String: BridgeReleaseUpload] = [
            uploadGroup.releaseId: BridgeReleaseUpload(
                progress: uploadGroup.progress,
                bar: BridgeUploadBar(
                    bytesDone: 22_400_000,
                    bytesTotal: 57_100_000
                )
            ),
            uploadGroupQueued.releaseId: BridgeReleaseUpload(
                progress: uploadGroupQueued.progress,
                bar: nil
            ),
        ]

        /// The upload queue's summary while `uploadGroups` are queued.
        static func outboxSnapshot(
            uploadGroups: [BridgeUploadReleaseGroup] = defaultUploadGroups,
            pauseState: BridgeOutboxPauseState = .running
        ) -> BridgeOutboxSnapshot {
            BridgeOutboxSnapshot(
                revision: 1,
                perRelease: Dictionary(
                    uniqueKeysWithValues: uploadGroups.map { group in
                        (group.releaseId, group.progress)
                    }
                ),
                pauseState: pauseState
            )
        }

        /// The bytes the preview's uploading files and release have moved, and
        /// how fast.
        static let uploadTransfers: BridgeOutboxTransferChange = .reset(
            transfers: [
                BridgeKeyedUploadTransfer(
                    key: .file(fileId: "release_files:f-2"),
                    transfer: BridgeUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 10_000_000,
                            bytesTotal: 31_000_000
                        ),
                        throughputBps: 1_000_000
                    )
                ),
                BridgeKeyedUploadTransfer(
                    key: .file(fileId: "release_files:rel-row-1-audio"),
                    transfer: BridgeUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 12_400_000,
                            bytesTotal: 26_100_000
                        ),
                        throughputBps: 2_200_000
                    )
                ),
                BridgeKeyedUploadTransfer(
                    key: .release(releaseId: "rel-row-1"),
                    transfer: BridgeUploadTransfer(
                        bar: BridgeUploadBar(
                            bytesDone: 22_400_000,
                            bytesTotal: 57_100_000
                        ),
                        throughputBps: 3_200_000
                    )
                ),
            ],
            total: BridgeUploadTransfer(
                bar: BridgeUploadBar(
                    bytesDone: 22_400_000,
                    bytesTotal: 57_100_000
                ),
                throughputBps: 6_800_000
            )
        )

        @MainActor
        static func outboxStore(
            _ snapshot: BridgeOutboxSnapshot = outboxSnapshot()
        ) -> OutboxStore {
            let store = OutboxStore(snapshot: snapshot)
            store.applyTransferChange(uploadTransfers)
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
                    return storageRow(
                        releaseId: "rel-row-\(index)",
                        albumId: "album-row-\(index)",
                        title: titles[(index - 1) % titles.count],
                        artist: artists[(index - 1) % artists.count],
                        year: index % 7 == 0 ? nil : Int32(1980 + index),
                        media: media[(index - 1) % media.count],
                        storageState: isRemote ? .remote : .local,
                        pinned: isRemote && index % 4 == 0,
                        transfer: index == 5 ? .pin : nil,
                        upload: rowUploads["rel-row-\(index)"],
                        fileCount: Int64(1 + index % 24),
                        totalSize: Int64(48_000_000 + index * 83_000_000)
                    )
                }
        }()

        private static func storageRow(
            releaseId: String,
            albumId: String,
            title: String,
            artist: String,
            year: Int32? = 2021,
            media: [BridgeMediaCount] = PreviewData.media(.cd),
            storageState: BridgeReleaseStorageState,
            pinned: Bool = false,
            transfer: BridgeReleaseStorageAction? = nil,
            upload: BridgeReleaseUpload? = nil,
            fileCount: Int64 = 12,
            totalSize: Int64 = 210_000_000
        ) -> BridgeStorageRow {
            BridgeStorageRow(
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
                )
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
            list.preloadForPreview(rows: rows)
            return list
        }

        /// A `Library` whose storage reads serve the fixture rows and nothing
        /// else — enough for `StorageManagerView`'s own `rebuildList()` to
        /// populate the table — and whose inspector shows each row's release
        /// with its entry in `downloads`, `outputs` and `uploadGroups`.
        static func storageLibrary(
            rows: [BridgeStorageRow] = storageRows,
            downloads: [BridgeDownloadOp] = downloadOps,
            outputs: [BridgeOutputOp] = outputOps,
            uploadGroups: [BridgeUploadReleaseGroup] = defaultUploadGroups
        ) -> Library {
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
                    upload: uploadGroups.first { $0.releaseId == detail.id }
                )
            }
            return Library(
                releaseDetail: { .fixed { details[$0] } },
                storageBrowse: { _, _ in .fixed(rows) },
                storageInspector: { .fixed { inspectors[$0] } }
            )
        }

        /// What core's inspector read delivers for `release` beside its
        /// queue entries, every queue running. The fixture uploads name no
        /// release file, so they list after the release's files.
        private static func storageInspector(
            release: BridgeRelease,
            download: BridgeDownloadOp?,
            output: BridgeOutputOp?,
            upload: BridgeUploadReleaseGroup?
        ) -> BridgeStorageInspector {
            var transfers: [BridgeStorageInspectorTransfer] = []
            if let download {
                transfers.append(.download(operation: download, paused: false))
            }
            if let output {
                transfers.append(.output(operation: output, paused: false))
            }
            if let upload {
                transfers.append(.upload(group: upload, paused: false))
            }
            return BridgeStorageInspector(
                transfers: transfers,
                files: release.files.map {
                    .releaseFile(file: $0, operation: nil)
                }
                    + (upload?.files ?? []).map { .upload(operation: $0) }
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
                    fileSize: 34_000_000,
                    contentType: "audio/flac",
                    isImage: false,
                    audioFormat: storageAudioFormat
                ),
                BridgeFile(
                    id: "\(row.id)-image",
                    originalFilename: "cover.jpg",
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
