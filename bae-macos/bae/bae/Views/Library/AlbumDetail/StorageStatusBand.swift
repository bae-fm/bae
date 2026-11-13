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
    /// release being pinned or moving to This Mac, each waiting its turn or moving.
    @ViewBuilder
    private var pill: some View {
        if release.summary.upload?.missing != nil {
            QueuedUploadChip()
        }
        else if let transfer {
            MacStorageChip(release: release.summary, status: transfer)
        }
    }

    /// The storage status of the change on its way, placed as
    /// `MacStorageChip` words it; nil when nothing is moving. Resting states
    /// and failures show elsewhere in the band. An upload held by a missing
    /// original shows as the queued upload beside the warning notice that
    /// names the file.
    private var transfer: BridgeStorageRowStatus? {
        if release.summary.upload != nil { return .uploading }
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
                title: String(localized: "Could not pin"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
        if case .failed(let error) = release.makeLocal {
            Notice(
                title: String(localized: "Could not move to This Mac"),
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
        HStack(spacing: ThemeSpace.compact) {
            switch release.summary.storageState {
            case .onOneDevice:
                Image(systemName: "folder").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("Local")
            case .inStorage(pinned: true):
                Image(systemName: "pin").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("Pinned")
            case .inStorage(pinned: false):
                Image(systemName: "cloud").themeIcon(.medium)
                    .accessibilityHidden(true)
                Text("In cloud")
            }
        }
        .themeText(.body)
        .foregroundStyle(.secondary)
    }

    private var transferActions: some View {
        FlowLayout(spacing: ThemeSpace.related) {
            ForEach(release.storageActions, id: \.self) { action in
                if action == .pin && pinQueued {
                    Button(
                        BridgeReleaseStorageAction.cancelPinningLabel,
                        action: onCancelPin
                    )
                }
                else {
                    Button(action.label) { onAction(action) }
                }
            }
            // Export and Save As change no state, so every release offers them.
            Button("Export…", action: onExport)
            Button("Save as…", action: onSaveAs)
        }
    }
}
