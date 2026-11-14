import BaeKit
import SwiftUI

/// The storage state chip: what core says the row's storage shows. Files on
/// the move fill their chip as far as they have come, as does a release
/// moving to This Mac.
struct MacStorageChip: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus

    var body: some View {
        switch status {
        case .downloadFailed(let error):
            StatusChip("Could not pin", tone: .danger)
                .help(error)
        case .moveFailed(let fileName):
            StatusChip(
                "Move failed: \(fileName) changed since import",
                tone: .danger
            )
        case .uploading:
            if let missing = release.upload?.missing {
                StatusChip("Waiting for \(missing)", tone: .warning)
            }
            else if release.upload?.progress.activity == .uploading {
                StorageUploadingChipContainer(releaseId: release.id)
            }
            else {
                QueuedUploadChip()
            }
        case .pinning(.queued):
            StatusChip("Queued to pin", tone: .activity)
        case .pinning(.running):
            StoragePinningChipContainer(releaseId: release.id)
        case .moveToThisDeviceFailed(let error):
            StatusChip("Could not move to This Mac", tone: .danger)
                .help(error)
        case .movingToThisDevice(.queued):
            StatusChip("Queued to move to This Mac", tone: .activity)
        case .movingToThisDevice(.running):
            StorageMovingToThisDeviceChipContainer(releaseId: release.id)
        case .onOneDevice:
            StatusChip("Local", symbol: "folder")
        case .pinned:
            StatusChip("Pinned", symbol: "pin")
        case .inStorage:
            StatusChip("In cloud", symbol: "cloud")
        }
    }
}

extension BridgeStorageRowStatus {
    /// Whether the release's move to the cloud failed.
    var isMoveFailed: Bool {
        if case .moveFailed = self { return true }
        return false
    }
}

/// A release whose files only wait to upload: nothing is moving to fill.
struct QueuedUploadChip: View {
    var body: some View {
        StatusChip(
            verbatim: BridgeUploadActivity.queued.phaseText,
            tone: .activity
        )
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
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StorageProgressChip(
            verbatim: BridgeReleaseStorageAction.moveToThisDevice.transferLabel,
            kind: .pin,
            fill: .moveToThisDevice(
                downloadStore.moveToThisDeviceProgress(forRelease: releaseId)
            )
        )
    }
}
