import BaeKit
import Foundation

@testable import bae

extension ShotScene {
    /// One of the board's Storage Manager rows.
    private struct BoardRow {
        let title: String
        let artist: String
        let media: [BridgeMediaCount]
        let state: BridgeReleaseStorageState
        let status: BridgeStorageRowStatus
        let files: Int64
        let megabytes: UInt64
        let added: String
    }

    private static let boardRows: [BoardRow] = [
        BoardRow(
            title: "Blue Room Sessions",
            artist: "Ada Lindqvist",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: false),
            status: .inStorage,
            files: 12,
            megabytes: 412,
            added: "2026-10-02"
        ),
        BoardRow(
            title: "Copper Sky",
            artist: "Odile Fen",
            media: PreviewData.media(.cd),
            state: .onOneDevice,
            status: .onOneDevice,
            files: 11,
            megabytes: 388,
            added: "2026-10-04"
        ),
        BoardRow(
            title: "Driftwood",
            artist: "Mara Quill",
            media: PreviewData.media(.vinyl),
            state: .inStorage(pinned: true),
            status: .pinned,
            files: 8,
            megabytes: 301,
            added: "2026-09-21"
        ),
        BoardRow(
            title: "Field Recordings",
            artist: "Mara Quill",
            media: PreviewData.media(.cd, 2),
            state: .onOneDevice,
            status: .uploading,
            files: 24,
            megabytes: 902,
            added: "2026-11-06"
        ),
        BoardRow(
            title: "Glass Harbor",
            artist: "Ines Varga Trio",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: false),
            status: .inStorage,
            files: 9,
            megabytes: 356,
            added: "2026-08-30"
        ),
        BoardRow(
            title: "Late Arrivals",
            artist: "The Slow Orchards",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: false),
            status: .pinning(step: .running),
            files: 10,
            megabytes: 344,
            added: "2026-10-12"
        ),
        BoardRow(
            title: "Low Tide Hours",
            artist: "Mara Quill",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: false),
            status: .inStorage,
            files: 10,
            megabytes: 402,
            added: "2026-07-14"
        ),
        BoardRow(
            title: "Northbound",
            artist: "Mara Quill",
            media: PreviewData.media(.cd),
            state: .onOneDevice,
            status: .onOneDevice,
            files: 12,
            megabytes: 415,
            added: "2026-11-01"
        ),
        BoardRow(
            title: "Paper Lanterns",
            artist: "The Slow Orchards",
            media: PreviewData.media(.vinyl),
            state: .inStorage(pinned: false),
            status: .inStorage,
            files: 9,
            megabytes: 288,
            added: "2026-06-03"
        ),
        BoardRow(
            title: "Salt and Cedar",
            artist: "The Slow Orchards",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: true),
            status: .pinned,
            files: 11,
            megabytes: 377,
            added: "2026-05-19"
        ),
        BoardRow(
            title: "Small Hours",
            artist: "Tomas Brell",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: false),
            status: .inStorage,
            files: 8,
            megabytes: 265,
            added: "2026-04-28"
        ),
        BoardRow(
            title: "Winter Kitchen",
            artist: "Odile Fen",
            media: PreviewData.media(.cd),
            state: .inStorage(pinned: true),
            status: .pinned,
            files: 10,
            megabytes: 331,
            added: "2026-03-02"
        ),
    ]

    /// The twelve releases the board's Storage Manager lists, release `n` as
    /// `rel-row-n` in the order the board lists them.
    static let boardStorageRows: [BridgeStorageRow] = boardRows.enumerated()
        .map { offset, row in
            let index = offset + 1
            let releaseId = "rel-row-\(index)"
            let albumId = "album-row-\(index)"
            let transitioning =
                row.status == .uploading
                || row.status == .pinning(step: .running)
            return BridgeStorageRow(
                release: BridgeReleaseSummary(
                    id: releaseId,
                    albumId: albumId,
                    media: row.media,
                    mediaTerms: bridgeMediaTerms(media: row.media),
                    storageState: row.state,
                    storageActions: [],
                    upload: nil,
                    transitioning: transitioning,
                    fileCount: row.files,
                    totalSize: row.megabytes * 1_000_000,
                    cover: nil
                ),
                album: BridgeAlbum(
                    id: albumId,
                    title: row.title,
                    year: nil,
                    isCompilation: false,
                    artistLine: .named(row.artist),
                    releaseIds: [releaseId],
                    primaryReleaseId: releaseId,
                    cover: nil
                ),
                status: row.status,
                dateAdded: ISO8601DateFormatter()
                    .date(from: "\(row.added)T12:00:00Z") ?? Date()
            )
        }
}
