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
/// This Mac as words, and Moving to This Mac… as a chip. A release whose upload
/// waits on a missing original shows Queued to upload, beside the notice that
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
                StorageWord(verbatim: BridgeUploadActivity.queued.phaseText)
            }
            else {
                StorageWord("Queued")
            }
        case .pinning(.queued):
            if long {
                StorageWord("Queued to pin")
            }
            else {
                StorageWord("Queued")
            }
        case .movingToThisDevice(.queued):
            if long {
                StorageWord("Queued to move to This Mac")
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
/// text, or a problem's tone colour, in the cell text role.
private struct StorageWord: View {
    let text: Text
    let tone: StatusTone?

    init(_ key: LocalizedStringKey, tone: StatusTone? = nil) {
        text = Text(key)
        self.tone = tone
    }

    init(verbatim string: String) {
        text = Text(verbatim: string)
        tone = nil
    }

    var body: some View {
        text
            .themeText(.cell)
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
            fill: .moveToThisDevice(
                downloadStore.moveToThisDeviceProgress(forRelease: releaseId)
            )
        )
    }
}
