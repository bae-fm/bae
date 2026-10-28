import BaeKit
import SwiftUI

/// The storage state chip: what core says the row's storage shows. Files on
/// the move fill their chip as far as they have come; a transition this
/// process runs while they rest shows its own words.
struct StorageStateLabel: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus

    var body: some View {
        switch status {
        case .uploading:
            StorageUploadingChip(releaseId: release.id)
        case .pinning:
            StoragePinningChip(releaseId: release.id)
        case .transferring(let action):
            StatusChip(verbatim: action.transferLabel, tone: .activity)
        case .local:
            StatusChip("Local", symbol: "folder")
        case .pinned:
            StatusChip("Pinned", symbol: "pin.fill")
        case .cloud:
            StatusChip("Cloud", symbol: "cloud")
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

#if DEBUG
    #Preview("Storage states") {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-local",
                    storageState: .local
                ),
                status: .local
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-cloud",
                    storageState: .remote,
                    pinned: false
                ),
                status: .cloud
            )
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-pinned",
                    storageState: .remote,
                    pinned: true
                ),
                status: .pinned
            )
            // A release downloading to stay on this device.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "rel-row-5",
                    storageState: .remote,
                    transfer: .pin
                ),
                status: .pinning
            )
            // A transition this process runs while the files rest.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: "r-detach",
                    storageState: .remote,
                    transfer: .detachAudio
                ),
                status: .transferring(action: .detachAudio)
            )
            // Files being sent.
            StorageStateLabel(
                release: PreviewData.storageRelease(
                    id: PreviewData.uploadingRelease.releaseId,
                    storageState: .local,
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
