import BaeKit
import SwiftUI

/// A release's storage state, download or upload progress, and storage
/// actions. An upload's and a make-local's bar and bytes are native views
/// that read the release's latest progress each frame, so moving bytes run
/// no SwiftUI update.
struct StorageStatusBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(DownloadStore.self)
    private var downloadStore

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            // A cloud transition in flight is the release's storage state
            // while it runs: its phase, bar, and count are one line.
            if let upload = release.summary.upload {
                if upload.progress.activity == .uploading {
                    uploadLine(upload.progress)
                        .themeText(.body)
                }
                else {
                    // Files only waiting: nothing is moving to measure.
                    HStack(spacing: ThemeSpace.related) {
                        UploadActivityLabel(phaseOf: upload.progress)
                            .themeText(.body)
                        if let missing = upload.missing {
                            StatusChip("Waiting for \(missing)", tone: .warning)
                        }
                    }
                }
            }
            else {
                storageStatus
            }
            if let download = release.downloadStatus {
                downloadLine(download)
            }
            // Why this Mac can't play the release: the device its audio is
            // on, or its originals missing here.
            if let unplayable = release.unplayableLine {
                Text(unplayable)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
            if let makeLocal = release.makeLocal {
                makeLocalLine(makeLocal)
            }
            else if !release.summary.transitioning {
                transferActions
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
    }

    /// The phase, then a bar and the bytes the release holds of its size,
    /// both read from the release's latest upload progress each frame.
    private func uploadLine(_ progress: BridgeUploadProgress) -> some View {
        let source = outboxStore.uploadProgress(forRelease: release.summary.id)
        return HStack(spacing: ThemeSpace.related) {
            UploadActivityLabel(phaseOf: progress)
                .lineLimit(1)
                .foregroundStyle(.secondary)
                .truncationMode(.middle)
                .layoutPriority(1)
            StreamBar(stream: .upload(source))
                .frame(minWidth: 48)
            StreamLabel(stream: .uploadBytes(source))
        }
    }

    /// Where the release stands in being made local: waiting its turn;
    /// downloading,
    /// the transition's words over the bytes the release and the file being
    /// downloaded hold and a bar of the release's, read from how far its
    /// files have downloaded into the person's folder each frame; or failed,
    /// a danger notice whose Details opens the error.
    @ViewBuilder
    private func makeLocalLine(_ status: BridgeMakeLocalStatus) -> some View {
        switch status {
        case .queued:
            StatusChip("Queued to make local", tone: .activity)
        case .making:
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                Text(BridgeReleaseStorageAction.detachAudio.transferLabel)
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                TransferReadout(
                    stream: .makeLocal(
                        downloadStore.makeLocalProgress(
                            forRelease: release.summary.id
                        )
                    ),
                    kind: .pin
                )
            }
        case .failed(let error):
            ErrorNotice(
                title: String(localized: "Couldn't make local"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
    }

    /// Where the release stands in the download queue: waiting, downloading,
    /// or failed, a danger notice whose Details opens the error; or, with no
    /// entry, being copied here by coven.
    @ViewBuilder
    private func downloadLine(_ status: BridgeReleaseDownloadStatus)
        -> some View
    {
        switch status {
        case .queued:
            StatusChip("Queued", tone: .activity)
        case .downloading:
            ProgressLine(
                BridgeReleaseStorageAction.pin.transferLabel,
                progress: nil
            )
            .themeText(.body)
        case .failed(let error):
            ErrorNotice(
                title: String(localized: "Download failed"),
                advice: nil,
                error: DisplayError(line: error)
            )
        case .copyingHere:
            StatusChip("Copying here", tone: .activity)
        case .downloaded, .available:
            EmptyView()
        }
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
                Button(action: { onAction(action) }) {
                    Label(
                        action.label(
                            keepingHere: release.summary.keepingHere
                        ),
                        systemImage: action.systemImage
                    )
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
