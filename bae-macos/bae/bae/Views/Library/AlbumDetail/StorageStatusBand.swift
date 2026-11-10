import BaeKit
import SwiftUI

/// A release's storage: the state it rests in and at most one pill for the
/// change on its way, a notice for what needs the person, and its storage
/// actions. The pill fills as far as the release's bytes have come, through
/// native views that read the release's latest progress each frame, so moving
/// bytes run no SwiftUI update.
struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onCancelPin: () -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            HStack(spacing: ThemeSpace.related) {
                storageStatus
                pill
            }
            if let missing = release.summary.upload?.missing {
                Notice(
                    title: String(localized: "\(missing) is missing"),
                    advice: String(
                        localized: "Restore it to finish uploading."
                    ),
                    error: nil,
                    tone: .warning
                )
            }
            failure
            // Why this Mac can't play the release: the device its audio is
            // on, or its originals missing here.
            if let unplayable = release.unplayableLine {
                Text(unplayable)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            if !release.summary.transitioning {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// The one change on its way: files uploading or waiting to upload, the
    /// release being pinned or made local, each waiting its turn or moving.
    @ViewBuilder
    private var pill: some View {
        if let upload = release.summary.upload {
            if upload.progress.activity == .uploading {
                StorageUploadingChip(releaseId: release.summary.id)
            }
            else {
                QueuedUploadChip()
            }
        }
        else if let download = release.downloadStatus {
            switch download {
            case .queued:
                StatusChip("Queued to pin", tone: .activity)
            case .downloading:
                StoragePinningChip(releaseId: release.summary.id)
            case .copyingHere:
                StorageProgressChip(label: "Pinning", kind: .pin, fill: nil)
            case .downloaded, .available, .failed:
                makeLocalPill
            }
        }
        else {
            makeLocalPill
        }
    }

    @ViewBuilder
    private var makeLocalPill: some View {
        switch release.makeLocal {
        case .queued:
            StatusChip("Queued to make local", tone: .activity)
        case .making:
            StorageMakingLocalChip(releaseId: release.summary.id)
        case .failed, nil:
            EmptyView()
        }
    }

    /// A pin or a make-local that failed: a danger notice whose Details opens
    /// the error.
    @ViewBuilder
    private var failure: some View {
        if case .failed(let error) = release.downloadStatus {
            Notice(
                title: String(localized: "Couldn't pin"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
        if case .failed(let error) = release.makeLocal {
            Notice(
                title: String(localized: "Couldn't make local"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
    }

    /// Whether the release's pin waits its turn, which Cancel pinning ends.
    private var pinQueued: Bool {
        if case .queued = release.downloadStatus { return true }
        return false
    }

    /// The resting storage state.
    private var storageStatus: some View {
        HStack(spacing: ThemeSpace.inline) {
            switch release.summary.storageState {
            case .onOneDevice:
                Image(systemName: "folder")
                Text("Local")
            case .inStorage(pinned: true):
                Image(systemName: "pin.fill")
                Text("Pinned")
            case .inStorage(pinned: false):
                Image(systemName: "cloud")
                Text("In storage")
            }
        }
        .themeText(.body)
        .foregroundStyle(.secondary)
    }

    private var transferActions: some View {
        Group {
            ForEach(release.storageActions, id: \.self) { action in
                if action == .pin && pinQueued {
                    Button(action: onCancelPin) {
                        Label(
                            BridgeReleaseStorageAction.cancelPinningLabel,
                            systemImage: "pin.slash"
                        )
                    }
                }
                else {
                    Button(action: { onAction(action) }) {
                        Label(
                            action.label(
                                keepingHere: release.summary.keepingHere
                            ),
                            systemImage: action.systemImage
                        )
                    }
                }
            }
            // Export and Save As change no state, so every release offers them.
            Button(action: { onExport() }) {
                Label("Export…", systemImage: "square.and.arrow.up")
            }
            Button(action: { onSaveAs() }) {
                Label("Save as…", systemImage: "square.and.arrow.down")
            }
        }
    }
}

#if DEBUG
    @MainActor
    private func previewStorageBand(
        storageState: BridgeReleaseStorageState,
        storageActions: [BridgeReleaseStorageAction],
        playability: BridgeReleasePlayability
    ) -> some View {
        StorageStatusBand(
            release: PreviewData.storageRelease(
                storageState: storageState,
                storageActions: storageActions,
                playability: playability
            ),
            onAction: { _ in },
            onCancelPin: {},
            onExport: {},
            onSaveAs: {}
        )
        .frame(width: 560)
        .background(Theme.background)
        .environment(OutboxStore(uploadsPaused: false))
        .environment(PreviewData.downloadStore())
    }

    // Local: the only offered transition is uploading to the cloud.
    #Preview("Storage Band — Local") {
        previewStorageBand(
            storageState: .onOneDevice,
            storageActions: [.upload],
            playability: .playable
        )
        .preferredColorScheme(.dark)
    }

    // On another device only: nothing to move or pin from here, and the
    // band names the device.
    #Preview("Storage Band — On Another Device") {
        previewStorageBand(
            storageState: .onOneDevice,
            storageActions: [],
            playability: .onOtherDevice(device: .active(name: "Ana's Mac"))
        )
        .preferredColorScheme(.dark)
    }

    // On a device that was removed from the library.
    #Preview("Storage Band — On a Removed Device") {
        previewStorageBand(
            storageState: .onOneDevice,
            storageActions: [],
            playability: .onOtherDevice(
                device: .former(name: "Ana's Mac", state: .removed)
            )
        )
        .preferredColorScheme(.dark)
    }

    // In the cloud, not pinned: can pin or pull back local.
    #Preview("Storage Band — Cloud") {
        previewStorageBand(
            storageState: .inStorage(pinned: false),
            storageActions: [.pin, .detachAudio],
            playability: .playable
        )
        .preferredColorScheme(.dark)
    }

    // Moved to the cloud to be kept here, its copies still downloading: can
    // stop keeping it or pull it back local.
    #Preview("Storage Band — Copying Here") {
        StorageStatusBand(
            release: PreviewData.storageRelease(
                storageState: .inStorage(pinned: false),
                storageActions: [.unpin, .detachAudio],
                keepingHere: true,
                playability: .playable
            ),
            onAction: { _ in },
            onCancelPin: {},
            onExport: {},
            onSaveAs: {}
        )
        .frame(width: 560)
        .background(Theme.background)
        .environment(OutboxStore(uploadsPaused: false))
        .environment(PreviewData.downloadStore())
        .preferredColorScheme(.dark)
    }

    // In the cloud and pinned: can unpin or pull back local.
    #Preview("Storage Band — Pinned") {
        previewStorageBand(
            storageState: .inStorage(pinned: true),
            storageActions: [.unpin, .detachAudio],
            playability: .playable
        )
        .preferredColorScheme(.dark)
    }
#endif
