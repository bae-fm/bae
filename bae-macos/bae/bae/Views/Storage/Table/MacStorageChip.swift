import BaeKit
import SwiftUI

/// The Storage column: one word of what core says the row's storage shows,
/// with no icon. Every state that stands still is a plain word in the row's
/// text: Local, Pinned and Cloud at rest, Queued for a change that has not
/// started, and, in their tone's colour, Error for a failure and Waiting for
/// an upload that needs a missing original; the tooltip says which and why.
/// Only a release whose files upload, pin or move to This Mac shows a chip,
/// filled by the share of bytes bae has transferred.
///
/// With `long`, which a release's Storage sheet passes, a change on its way
/// names what it does: Queued to upload, Queued to pin, Queued to move to
/// This Mac and Moving to This Mac…, each a chip. A release whose upload
/// waits on a missing original shows the queued chip, beside the notice that
/// names the file.
struct MacStorageChip: View {
    let release: ReleaseSummary
    let status: BridgeStorageRowStatus
    var long = false

    var body: some View {
        switch status {
        case .downloadFailed(let error), .moveToThisDeviceFailed(let error):
            StorageWord("Error", tone: .danger)
                .help(error)
        case .moveFailed(let fileName):
            StorageWord("Error", tone: .danger)
                .help(
                    String(
                        localized:
                            "Could not move: \(fileName) changed after you imported it."
                    )
                )
        case .uploading:
            if let missing = release.upload?.missing, !long {
                StorageWord("Waiting", tone: .warning)
                    .help(String(localized: "Waiting for \(missing)"))
            }
            else if release.upload?.missing == nil,
                release.upload?.progress.activity == .uploading
            {
                StorageUploadingChipContainer(releaseId: release.id)
            }
            else if long {
                StatusChip(
                    verbatim: BridgeUploadActivity.queued.phaseText,
                    tone: .activity
                )
            }
            else {
                StorageWord("Queued")
            }
        case .pinning(.queued):
            if long {
                StatusChip("Queued to pin", tone: .activity)
            }
            else {
                StorageWord("Queued")
            }
        case .movingToThisDevice(.queued):
            if long {
                StatusChip("Queued to move to This Mac", tone: .activity)
            }
            else {
                StorageWord("Queued")
            }
        case .pinning(.running):
            StoragePinningChipContainer(releaseId: release.id)
        case .movingToThisDevice(.running):
            StorageMovingToThisDeviceChipContainer(
                releaseId: release.id,
                label: long
                    ? Text(
                        verbatim: BridgeReleaseStorageAction.moveToThisDevice
                            .transferLabel
                    )
                    : Text("Moving")
            )
        case .onOneDevice:
            StorageWord("Local")
        case .pinned:
            StorageWord("Pinned")
        case .inStorage:
            StorageWord("Cloud")
        }
    }
}

/// A storage state that stands still, as a word in the row's text: primary
/// text, or a problem's tone colour. The design system draws it at 11 points
/// regular, a size its text roles don't name.
private struct StorageWord: View {
    let key: LocalizedStringKey
    let tone: StatusTone?

    init(_ key: LocalizedStringKey, tone: StatusTone? = nil) {
        self.key = key
        self.tone = tone
    }

    var body: some View {
        Text(key)
            .font(.system(size: 11))
            .lineLimit(1)
            .foregroundStyle(tone?.color ?? Theme.textPrimary)
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
