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
        storageCell(
            "mac.component.storage-cell.local",
            glassHarborRow(1, state: .onOneDevice, status: .onOneDevice),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.cloud",
            glassHarborRow(1),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.pinning",
            glassHarborRow(1, status: .pinning(step: .running)),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.detaching",
            glassHarborRow(1, status: .movingToThisDevice(step: .running)),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.queued",
            glassHarborRow(1, status: .pinning(step: .queued)),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.upload-queued",
            fieldRecordingsRow(1),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.move-to-this-device-queued",
            glassHarborRow(1, status: .movingToThisDevice(step: .queued)),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.upload-blocked",
            fieldRecordingsRow(1),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.move-failed",
            glassHarborRow(
                1,
                status: .moveFailed(fileName: "01 Harbor Lights.flac")
            ),
            .storage
        ),
        storageCell(
            "mac.component.storage-cell.move-to-this-device-failed",
            glassHarborRow(
                1,
                status: .moveToThisDeviceFailed(
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
