import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// `MacStorageCell`: an artist, a size at the end of its column, and
    /// the storage column pinned, uploading and failed to pin.
    static let macStorageCellScenes: [ShotScene] = [
        storageCell(
            "mac.component.storage-cell.text",
            glassHarborRow(1),
            .artist
        ),
        storageCell(
            "mac.component.storage-cell.number",
            glassHarborRow(1),
            .size,
            width: 100
        ),
        storageCell(
            "mac.component.storage-cell.storage-at-rest",
            glassHarborRow(1, state: .inStorage(pinned: true), status: .pinned),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.storage-moving",
            fieldRecordingsRow(2),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.storage-failed",
            glassHarborRow(
                1,
                status: .downloadFailed(
                    error: "The network connection was lost."
                )
            ),
            .storage
        ),
    ]

    /// `MacStorageCell`: `row`'s cell in `column`, `width` points wide, over
    /// the upload of Field Recordings (`rel-shot-2`), 62 percent sent.
    private static func storageCell(
        _ id: String,
        _ row: BridgeStorageRow,
        _ column: StorageTableColumn,
        width: CGFloat = 380
    ) -> ShotScene {
        component(id, height: 40) {
            MacStorageCell(
                release: ReleaseSummary(from: row.release),
                album: row.album,
                status: row.status,
                dateAdded: row.dateAdded,
                column: column
            )
            .frame(width: width, alignment: .leading)
            .environment(uploadingOutbox("rel-shot-2"))
            .environment(DownloadStore())
        }
    }
}
