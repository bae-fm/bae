import BaeKit
import SwiftUI

/// The Storage column's chip: one word of what core says the row's storage
/// shows, with no icon. While files upload, pin or move to This Mac, the chip
/// fills by the share of bytes bae has transferred. The chip shows Queued for
/// a change that has not started, Error for a failure, and Waiting for an
/// upload that needs a missing original; the tooltip says which and why.
struct MacStorageChip: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus

    var body: some View {
        switch status {
        case .downloadFailed(let error), .moveToThisDeviceFailed(let error):
            StatusChip("Error", tone: .danger)
                .help(error)
        case .moveFailed(let fileName):
            StatusChip("Error", tone: .danger)
                .help(
                    String(
                        localized:
                            "Could not move: \(fileName) changed after you imported it."
                    )
                )
        case .uploading:
            if let missing = release.upload?.missing {
                StatusChip("Waiting", tone: .warning)
                    .help(String(localized: "Waiting for \(missing)"))
            }
            else if release.upload?.progress.activity == .uploading {
                StorageUploadingChipContainer(releaseId: release.id)
            }
            else {
                StatusChip("Queued", tone: .activity)
            }
        case .pinning(.queued), .movingToThisDevice(.queued):
            StatusChip("Queued", tone: .activity)
        case .pinning(.running):
            StoragePinningChipContainer(releaseId: release.id)
        case .movingToThisDevice(.running):
            StorageMovingToThisDeviceChipContainer(
                releaseId: release.id,
                label: Text("Moving")
            )
        case .onOneDevice:
            StatusChip("Local")
        case .pinned:
            StatusChip("Pinned")
        case .inStorage:
            StatusChip("Cloud")
        }
    }
}

/// An uploading release's chip, filled each frame from the release's latest
/// upload progress while any file is being sent.
struct StorageUploadingChipContainer: View {
    let releaseId: String
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        StorageProgressChip(
            label: "Uploading",
            kind: .upload,
            fill: .upload(outboxStore.uploadProgress(forRelease: releaseId))
        )
    }
}

/// A pinning release's chip, filled each frame from the release's latest
/// download progress; a release still waiting its turn has moved nothing.
struct StoragePinningChipContainer: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StorageProgressChip(
            label: "Pinning",
            kind: .pin,
            fill: .pin(downloadStore.downloadProgress(forRelease: releaseId))
        )
    }
}

/// A release moving to This Mac's chip, filled each frame from how far its
/// files have downloaded into the person's folder.
struct StorageMovingToThisDeviceChipContainer: View {
    let releaseId: String
    let label: Text
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StorageProgressChip(
            text: label,
            kind: .pin,
            fill: .moveToThisDevice(
                downloadStore.moveToThisDeviceProgress(forRelease: releaseId)
            )
        )
    }
}
