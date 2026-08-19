#if DEBUG
    import BaeKit
    import SwiftUI

    private final class PreviewStorageSubscription: LiveSubscriptionProtocol,
        @unchecked Sendable
    {
        func cancel() {}
    }

    // Preview fixtures for the Storage Manager: its transfer/sync queue rows
    // (downloads, exports, cloud-outbox uploads and deletes) and its release /
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

        /// One active, one queued, one failed — the three download-row states.
        static let downloadOps: [BridgeDownloadOp] = [
            BridgeDownloadOp(
                releaseId: "rel-dl-1",
                title: "Album Title A",
                fileCount: 12,
                totalSize: 367_000_000,
                createdAt: queuedAt(minutesAgo: 1),
                state: .active(
                    progress: BridgeDownloadTransferProgress(
                        bytesDone: 140_000_000,
                        bytesTotal: 367_000_000,
                        fraction: 140.0 / 367.0
                    )
                )
            ),
            BridgeDownloadOp(
                releaseId: "rel-dl-2",
                title: "Album Title B",
                fileCount: 9,
                totalSize: 210_000_000,
                createdAt: queuedAt(minutesAgo: 3),
                state: .queued
            ),
            BridgeDownloadOp(
                releaseId: "rel-dl-3",
                title: "Album Title C",
                fileCount: 15,
                totalSize: 512_000_000,
                createdAt: queuedAt(minutesAgo: 8),
                state: .failed(error: "The network connection was lost.")
            ),
        ]

        static func downloadSnapshot(
            ops: [BridgeDownloadOp] = downloadOps,
            paused: Bool = false
        ) -> BridgeDownloadSnapshot {
            BridgeDownloadSnapshot(
                downloads: ops,
                total: BridgeDownloadProgress(queued: 1, active: 1, failed: 1),
                summaryParts: [
                    BridgeCountLabel(key: "core.queue.downloading", count: 1),
                    BridgeCountLabel(key: "core.queue.failed", count: 1),
                    BridgeCountLabel(key: "core.queue.queued", count: 1),
                ],
                paused: paused
            )
        }

        @MainActor
        static func downloadStore(
            _ snapshot: BridgeDownloadSnapshot = downloadSnapshot()
        ) -> DownloadStore {
            DownloadStore(snapshot: snapshot)
        }

        // MARK: - Export / save queue

        /// One active save, one queued export, one failed export — the output-row
        /// states across both kinds.
        static let outputOps: [BridgeOutputOp] = [
            BridgeOutputOp(
                releaseId: "rel-out-1",
                targetDir: "/Music/Exports",
                title: "Album Title A",
                fileCount: 12,
                totalSize: 213_000_000,
                createdAt: queuedAt(minutesAgo: 1),
                state: .active(percent: 62),
                kind: .save(presetName: "FLAC")
            ),
            BridgeOutputOp(
                releaseId: "rel-out-2",
                targetDir: "/Music/Exports",
                title: "Album Title B",
                fileCount: 9,
                totalSize: 340_000_000,
                createdAt: queuedAt(minutesAgo: 4),
                state: .queued,
                kind: .export
            ),
            BridgeOutputOp(
                releaseId: "rel-out-3",
                targetDir: "/Music/Exports",
                title: "Album Title C",
                fileCount: 15,
                totalSize: 512_000_000,
                createdAt: queuedAt(minutesAgo: 9),
                state: .failed(error: "Destination folder is not writable."),
                kind: .export
            ),
        ]

        static func outputSnapshot(
            ops: [BridgeOutputOp] = outputOps,
            paused: Bool = false
        ) -> BridgeOutputSnapshot {
            BridgeOutputSnapshot(
                outputs: ops,
                total: BridgeOutputProgress(queued: 1, active: 1, failed: 1),
                summaryParts: [
                    BridgeCountLabel(key: "core.queue.output", count: 1),
                    BridgeCountLabel(key: "core.queue.failed", count: 1),
                    BridgeCountLabel(key: "core.queue.queued", count: 1),
                ],
                paused: paused
            )
        }

        @MainActor
        static func outputStore(
            _ snapshot: BridgeOutputSnapshot = outputSnapshot()
        ) -> OutputStore {
            OutputStore(snapshot: snapshot)
        }

        // MARK: - Cloud outbox (uploads + deletes)

        /// Every durable and transient upload-file phase.
        static let uploadFileOps: [BridgeUploadFileOp] = [
            BridgeUploadFileOp(
                fileId: "f-1",
                label: .cover,
                bar: nil,
                sourceBytesTotal: 24_000_000,
                state: .uploaded,
                lastError: nil
            ),
            BridgeUploadFileOp(
                fileId: "f-2",
                label: .filename(name: "02 Track Title.flac"),
                bar: BridgeUploadBar(
                    phase: .preparing,
                    bytesDone: 10_000_000,
                    bytesTotal: 31_000_000
                ),
                sourceBytesTotal: 31_000_000,
                state: .preparing,
                lastError: nil
            ),
            BridgeUploadFileOp(
                fileId: "f-3",
                label: .filename(name: "03 Track Title.flac"),
                bar: nil,
                sourceBytesTotal: 28_000_000,
                state: .prepared,
                lastError: nil
            ),
            BridgeUploadFileOp(
                fileId: "f-4",
                label: .filename(name: "04 Track Title.flac"),
                bar: BridgeUploadBar(
                    phase: .uploading,
                    bytesDone: 12_400_000,
                    bytesTotal: 26_100_000
                ),
                sourceBytesTotal: 26_000_000,
                state: .uploading,
                lastError: nil
            ),
            BridgeUploadFileOp(
                fileId: "f-5",
                label: .filename(name: "05 Track Title.flac"),
                bar: nil,
                sourceBytesTotal: 22_000_000,
                state: .retrying,
                lastError: "Upload timed out; will retry."
            ),
            BridgeUploadFileOp(
                fileId: "f-6",
                label: .filename(name: "06 Track Title.flac"),
                bar: nil,
                sourceBytesTotal: 18_000_000,
                state: .queued,
                lastError: nil
            ),
        ]

        static func uploadProgress(
            activity: BridgeUploadActivity?
        ) -> BridgeUploadProgress {
            BridgeUploadProgress(
                queued: 1,
                preparing: 1,
                prepared: 1,
                uploading: 1,
                retrying: 1,
                uploaded: 1,
                publishing: 0,
                cancelling: 0,
                bar: BridgeUploadBar(
                    phase: .preparing,
                    bytesDone: 93_000_000,
                    bytesTotal: 149_000_000
                ),
                activity: activity,
                canCancel: true
            )
        }

        static let uploadGroup = BridgeUploadReleaseGroup(
            releaseId: "rel-up-1",
            displayTitle: "Album Title A",
            files: uploadFileOps,
            progress: uploadProgress(activity: .uploading)
        )

        /// A second group whose blobs landed and whose release is publishing.
        static let uploadGroupDone = BridgeUploadReleaseGroup(
            releaseId: "rel-up-2",
            displayTitle: "Album Title B",
            files: [
                BridgeUploadFileOp(
                    fileId: "g-1",
                    label: .filename(name: "01 Track Title.flac"),
                    bar: nil,
                    sourceBytesTotal: 18_000_000,
                    state: .uploaded,
                    lastError: nil
                )
            ],
            progress: BridgeUploadProgress(
                queued: 0,
                preparing: 0,
                prepared: 0,
                uploading: 0,
                retrying: 0,
                uploaded: 1,
                publishing: 1,
                cancelling: 0,
                bar: BridgeUploadBar(
                    phase: .uploading,
                    bytesDone: 18_100_000,
                    bytesTotal: 18_100_000
                ),
                activity: .publishing,
                canCancel: false
            )
        )

        static let deleteOps: [BridgeDeleteOp] = [
            BridgeDeleteOp(
                namespace: "release_files",
                blobId: "8b1f0f2e-2a52-45b2-9d19-3c0a1e6b4d77",
                createdAt: queuedAt(minutesAgo: 2)
            ),
            BridgeDeleteOp(
                namespace: "covers",
                blobId: "c4a7d3f1-6e88-4b90-8a02-5f1de9c3b210",
                createdAt: queuedAt(minutesAgo: 6)
            ),
        ]

        static func outboxSnapshot(
            uploadGroups: [BridgeUploadReleaseGroup] = [
                uploadGroup, uploadGroupDone,
            ],
            deletes: [BridgeDeleteOp] = deleteOps,
            pauseState: BridgeOutboxPauseState = .running
        ) -> BridgeOutboxSnapshot {
            let perRelease = Dictionary(
                uniqueKeysWithValues: uploadGroups.map { group in
                    (group.releaseId, group.progress)
                }
            )
            return BridgeOutboxSnapshot(
                revision: 1,
                uploadGroups: uploadGroups,
                deletes: deletes,
                perRelease: perRelease,
                total: BridgeUploadProgress(
                    queued: 1,
                    preparing: 1,
                    prepared: 1,
                    uploading: 1,
                    retrying: 1,
                    uploaded: 2,
                    publishing: 1,
                    cancelling: 0,
                    bar: BridgeUploadBar(
                        phase: .preparing,
                        bytesDone: 111_000_000,
                        bytesTotal: 167_000_000
                    ),
                    activity: .uploading,
                    canCancel: false
                ),
                pendingDeletes: UInt32(deletes.count),
                summaryParts: [
                    BridgeCountLabel(key: "core.queue.uploading", count: 1),
                    BridgeCountLabel(key: "core.outbox.retrying", count: 1),
                    BridgeCountLabel(key: "core.queue.queued", count: 1),
                    BridgeCountLabel(
                        key: "core.outbox.pending_deletes",
                        count: UInt32(deletes.count)
                    ),
                ],
                pauseState: pauseState,
                throughputBps: 6_800_000,
                etaSeconds: 42
            )
        }

        @MainActor
        static func outboxStore(
            _ snapshot: BridgeOutboxSnapshot = outboxSnapshot()
        ) -> OutboxStore {
            OutboxStore(snapshot: snapshot)
        }

        // MARK: - Storage table rows

        /// A release summary in a chosen storage state, built through the wire
        /// type so `ReleaseSummary`'s real projection runs. `transfer` seeds an
        /// in-flight transition badge.
        @MainActor
        static func storageRelease(
            id: String = "rel-store-1",
            albumId: String = "album-store-1",
            format: String? = "FLAC",
            storageState: BridgeReleaseStorageState = .remote,
            pinned: Bool = false,
            transfer: BridgeReleaseStorageAction? = nil,
            fileCount: Int64 = 12,
            totalSize: Int64 = 367_000_000
        ) -> ReleaseSummary {
            ReleaseSummary(
                from: BridgeReleaseSummary(
                    id: id,
                    albumId: albumId,
                    format: format,
                    storageState: storageState,
                    pinned: pinned,
                    storageActions: [],
                    transferAction: transfer,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    cover: nil
                )
            )
        }

        static let storageAlbum = AlbumSummary(
            from: BridgeAlbum(
                id: "album-store-1",
                title: "Album Title",
                year: 2021,
                isCompilation: false,
                artistNames: "Artist Name",
                releaseIds: ["rel-store-1"],
                primaryReleaseId: "rel-store-1",
                cover: nil
            )
        )

        /// An audio file row (carries a format descriptor) and an image file row
        /// (no descriptor) — the two `StorageFileCell` shapes.
        static let storageAudioFile = BridgeFile(
            id: "file-audio-1",
            originalFilename: "01 Track Title.flac",
            fileSize: 34_000_000,
            contentType: "audio/flac",
            isImage: false,
            audioFormat: BridgeAudioFormat(
                codec: "FLAC",
                sampleRateHz: 44_100,
                bitsPerSample: 16,
                bitrateKbps: nil,
                channels: 2
            )
        )

        static let storageImageFile = BridgeFile(
            id: "file-image-1",
            originalFilename: "cover.jpg",
            fileSize: 2_400_000,
            contentType: "image/jpeg",
            isImage: true,
            audioFormat: nil
        )

        // MARK: - Whole-screen: seeded list + Library

        /// The storage rows the table renders: one local, one cloud, one
        /// pinned. Album summaries share one album id per row for the join.
        static let storageRows: [BridgeStorageRow] = [
            storageRow(
                releaseId: "rel-row-1",
                albumId: "album-row-1",
                title: "Album Title A",
                artist: "Artist Name A",
                storageState: .local,
                pinned: false,
                fileCount: 10,
                totalSize: 280_000_000
            ),
            storageRow(
                releaseId: "rel-row-2",
                albumId: "album-row-2",
                title: "Album Title B",
                artist: "Artist Name B",
                storageState: .remote,
                pinned: false,
                fileCount: 12,
                totalSize: 367_000_000
            ),
            storageRow(
                releaseId: "rel-row-3",
                albumId: "album-row-3",
                title: "Album Title C",
                artist: "Artist Name C",
                storageState: .remote,
                pinned: true,
                fileCount: 9,
                totalSize: 210_000_000
            ),
        ]

        private static func storageRow(
            releaseId: String,
            albumId: String,
            title: String,
            artist: String,
            storageState: BridgeReleaseStorageState,
            pinned: Bool = false,
            fileCount: Int64 = 12,
            totalSize: Int64 = 210_000_000
        ) -> BridgeStorageRow {
            BridgeStorageRow(
                release: BridgeReleaseSummary(
                    id: releaseId,
                    albumId: albumId,
                    format: "FLAC",
                    storageState: storageState,
                    pinned: pinned,
                    storageActions: [],
                    transferAction: nil,
                    fileCount: fileCount,
                    totalSize: totalSize,
                    cover: nil
                ),
                album: BridgeAlbum(
                    id: albumId,
                    title: title,
                    year: 2021,
                    isCompilation: false,
                    artistNames: artist,
                    releaseIds: [releaseId],
                    primaryReleaseId: releaseId,
                    cover: nil
                )
            )
        }

        /// A pre-populated `StorageList` for the footer and whole-screen
        /// previews — interns each row into `store` and seeds the ids so the
        /// table paints without an async round-trip.
        @MainActor
        static func storageList(
            rows: [BridgeStorageRow] = storageRows,
            store: LibraryStore
        ) -> StorageList {
            for row in rows {
                _ = store.internAlbumSummary(row.album)
                _ = store.internReleaseSummary(row.release)
            }
            let list = StorageList(
                pageSource: storageLibrary(rows: rows).storagePageSource,
                ingest: { rows in
                    for row in rows {
                        _ = store.internAlbumSummary(row.album)
                        _ = store.internReleaseSummary(row.release)
                    }
                },
                onError: { _ in }
            )
            list.preloadForPreview(ids: rows.map(\.id))
            return list
        }

        /// A `Library` whose storage reads serve the fixture rows and nothing
        /// else — enough for `StorageManagerView`'s own `rebuildList()` to
        /// populate the table.
        static func storageLibrary(
            rows: [BridgeStorageRow] = storageRows
        ) -> Library {
            Library(
                subscribeStorageProjection: { _, _, offset, limit, callback in
                    let start = min(Int(offset), rows.count)
                    let end = min(start + Int(limit), rows.count)
                    callback.onValue(
                        value: BridgeStorageProjection(
                            page: BridgeStoragePage(
                                rows: Array(rows[start..<end]),
                                totalCount: UInt64(rows.count)
                            ),
                            totalSize: UInt64(
                                rows.reduce(0) {
                                    $0 + $1.release.totalSize
                                }
                            )
                        )
                    )
                    return PreviewStorageSubscription()
                }
            )
        }
    }

    extension Library {
        /// The storage page source over this library — the seam
        /// `StorageManagerView` builds internally, exposed for the preview list.
        fileprivate var storagePageSource: StoragePageSource {
            StoragePageSource(
                library: self,
                sort: BridgeStorageSort(
                    field: .albumTitle,
                    direction: .ascending
                ),
                filter: .all,
                onTotalSize: { _ in }
            )
        }
    }
#endif
