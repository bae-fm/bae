import BaeKit
import SwiftUI

/// The storage state chip: what core says the row's storage shows. Files on
/// the move fill their chip as far as they have come, as does a release
/// being made local.
struct MacStorageChip: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus

    var body: some View {
        switch status {
        case .downloadFailed(let error):
            StatusChip("Couldn’t pin", tone: .danger)
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
        case .makeLocalFailed(let error):
            StatusChip("Couldn’t make local", tone: .danger)
                .help(error)
        case .makingLocal(.queued):
            StatusChip("Queued to make local", tone: .activity)
        case .makingLocal(.running):
            StorageMakingLocalChipContainer(releaseId: release.id)
        case .onOneDevice:
            StatusChip("Local", symbol: "folder")
        case .pinned:
            StatusChip("Pinned", symbol: "pin.fill")
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

/// A release being made local's chip, filled each frame from how far its
/// files have downloaded into the person's folder.
struct StorageMakingLocalChipContainer: View {
    let releaseId: String
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        StorageProgressChip(
            verbatim: BridgeReleaseStorageAction.detachAudio.transferLabel,
            kind: .pin,
            fill: .makeLocal(
                downloadStore.makeLocalProgress(forRelease: releaseId)
            )
        )
    }
}

#if DEBUG
    #Preview("Storage states") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-local",
                    storageState: .onOneDevice
                ),
                status: .onOneDevice
            )
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-cloud",
                    storageState: .inStorage(pinned: false)
                ),
                status: .inStorage
            )
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-pinned",
                    storageState: .inStorage(pinned: true)
                ),
                status: .pinned
            )
            // A release downloading to stay on this device.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "rel-row-5",
                    storageState: .inStorage(pinned: false),
                    transitioning: true
                ),
                status: .pinning(step: .running)
            )
            // A pin waiting its turn.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-queued",
                    storageState: .inStorage(pinned: false)
                ),
                status: .pinning(step: .queued)
            )
            // A download that failed.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-failed",
                    storageState: .inStorage(pinned: false)
                ),
                status: .downloadFailed(
                    error: "The network connection was lost"
                )
            )
            // A move whose original changed.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-move-failed",
                    storageState: .onOneDevice
                ),
                status: .moveFailed(fileName: "01 Opening.flac")
            )
            // A release downloading into the person's folder to be made local.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-detach",
                    storageState: .inStorage(pinned: false),
                    transitioning: true
                ),
                status: .makingLocal(step: .running)
            )
            // A make-local waiting its turn.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-make-local-queued",
                    storageState: .inStorage(pinned: false),
                    transitioning: true
                ),
                status: .makingLocal(step: .queued)
            )
            // A make-local that failed.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: "r-make-local-failed",
                    storageState: .inStorage(pinned: false)
                ),
                status: .makeLocalFailed(error: "The disk is full")
            )
            // Files being sent.
            MacStorageChip(
                release: PreviewData.storageRelease(
                    id: PreviewData.uploadingRelease.releaseId,
                    storageState: .onOneDevice,
                    upload: PreviewData.uploadingRelease.upload
                ),
                status: .uploading
            )
        }
        .padding()
        .environment(PreviewData.downloadStore())
        .environment(PreviewData.outboxStore())
    }
#endif
