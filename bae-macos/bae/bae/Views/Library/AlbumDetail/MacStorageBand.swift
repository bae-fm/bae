import BaeKit
import SwiftUI

/// A release's storage: the state it rests in and at most one pill for the
/// change on its way, a notice for what needs the person, and its storage
/// actions. The pill fills as far as the release's bytes have come, through
/// native views that read the release's latest progress each frame, so moving
/// bytes run no SwiftUI update.
struct MacStorageBand: View {
    let release: ReleaseDetail
    let onAction: (BridgeReleaseStorageAction) -> Void
    let onCancelPin: () -> Void
    let onExport: () -> Void
    let onSaveAs: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.group) {
            HStack(spacing: ThemeSpace.related) {
                MacStorageState(state: release.summary.storageState)
                pill
            }
            if let missing = release.summary.upload?.missing {
                MacNotice(
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
        if let transfer {
            MacStorageChip(
                release: release.summary,
                status: transfer,
                long: true
            )
        }
    }

    /// The storage status of the change on its way, drawn by
    /// `MacStorageChip` with `long`; nil when nothing is moving. Resting states
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
        switch release.moveToThisDevice {
        case .queued: return .movingToThisDevice(step: .queued)
        case .making: return .movingToThisDevice(step: .running)
        case .failed, nil: return nil
        }
    }

    /// A pin or a move-to-this-device that failed: a danger notice whose Details opens
    /// the error.
    @ViewBuilder
    private var failure: some View {
        if case .failed(let error) = release.downloadStatus {
            MacNotice(
                title: String(localized: "Could not pin"),
                advice: nil,
                error: DisplayError(line: error)
            )
        }
        if case .failed(let error) = release.moveToThisDevice {
            MacNotice(
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

    private var transferActions: some View {
        FlowLayout(spacing: ThemeSpace.related) {
            ForEach(release.storageActions, id: \.self) { action in
                if action == .pin && pinQueued {
                    MacButton(
                        verbatim: BridgeReleaseStorageAction.cancelPinningLabel,
                        action: onCancelPin
                    )
                }
                else {
                    MacButton(verbatim: action.label) { onAction(action) }
                }
            }
            // Export and Save As change no state, so every release offers them.
            MacButton("Export…", action: onExport)
            MacButton("Save as…", action: onSaveAs)
        }
    }
}
