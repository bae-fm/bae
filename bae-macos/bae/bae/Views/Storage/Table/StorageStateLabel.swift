import BaeKit
import SwiftUI

/// The storage state chip: what core says the row's storage shows. Files on
/// the move fill their chip as far as they have come, as does a release
/// being made local; another transition this process runs while the files
/// rest shows its own words.
struct StorageStateLabel: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus

    var body: some View {
        switch status {
        case .uploading:
            StorageUploadingChip(releaseId: release.id)
        case .pinning:
            StoragePinningChip(releaseId: release.id)
        case .transferring(.detachAudio):
            StorageMakingLocalChip(releaseId: release.id)
        case .transferring(let action):
            StatusChip(verbatim: action.transferLabel, tone: .activity)
        case .local:
            StatusChip("Local", symbol: "folder")
        case .pinned:
            StatusChip("Pinned", symbol: "pin.fill")
        case .cloud:
            StatusChip("In storage", symbol: "cloud")
        }
    }
}

/// An uploading release's chip, filled each frame from the release's latest
/// upload progress while any file is being sent.
private struct StorageUploadingChip: View {
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
private struct StoragePinningChip: View {
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
private struct StorageMakingLocalChip: View {
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
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-local",
                    storageState: .onOneDevice
                ),
                status: .local
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-cloud",
                    storageState: .inStorage(pinned: false)
                ),
                status: .cloud
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-pinned",
                    storageState: .inStorage(pinned: true)
                ),
                status: .pinned
            )
            // A release downloading to stay on this device.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "rel-row-5",
                    storageState: .inStorage(pinned: false),
                    transfer: .pin
                ),
                status: .pinning
            )
            // A transition this process runs while the files rest.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-detach",
                    storageState: .inStorage(pinned: false),
                    transfer: .detachAudio
                ),
                status: .transferring(action: .detachAudio)
            )
            // Files being sent.
            StorageStateLabel(
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
