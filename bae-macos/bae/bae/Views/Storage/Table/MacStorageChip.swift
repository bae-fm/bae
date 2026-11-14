import BaeKit
import SwiftUI

/// The Storage column's chip: one word of what core says the row's storage
/// shows, with no icon. Files on the move fill their chip as far as they have
/// come, as does a release moving to This Mac. A change that waits its turn
/// reads Queued, a failure Error, and an upload held by a missing original
/// Waiting; the tooltip says which and why.
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
                            "Move failed: \(fileName) changed since import"
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
