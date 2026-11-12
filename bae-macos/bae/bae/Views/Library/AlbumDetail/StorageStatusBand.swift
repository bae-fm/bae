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
        if let transfer {
            MacStorageChip(release: release.summary, status: transfer)
        }
    }

    /// The storage status of the change on its way, placed as
    /// `MacStorageChip` words it; nil when nothing is moving. Resting states
    /// and failures show elsewhere in the band, and a file whose original is
    /// missing shows as the warning notice alone, so no chip repeats it.
    private var transfer: BridgeStorageRowStatus? {
        if let upload = release.summary.upload {
            return upload.missing == nil ? .uploading : nil
        }
        if let download = release.downloadStatus {
            switch download {
            case .queued: return .pinning(step: .queued)
            case .downloading: return .pinning(step: .running)
            case .downloaded, .available, .failed: break
            }
        }
        switch release.makeLocal {
        case .queued: return .makingLocal(step: .queued)
        case .making: return .makingLocal(step: .running)
        case .failed, nil: return nil
        }
    }

    /// A pin or a make-local that failed: a danger notice whose Details opens
    /// the error.
    @ViewBuilder
    private var failure: some View {
        if case .failed(let error) = release.downloadStatus {
            Notice(
                title: String(localized: "Couldn’t pin"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
        if case .failed(let error) = release.makeLocal {
            Notice(
                title: String(localized: "Couldn’t make local"),
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
                Text("In cloud")
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
                        Label(action.label, systemImage: action.systemImage)
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
