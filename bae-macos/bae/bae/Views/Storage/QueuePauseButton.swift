import BaeKit
import SwiftUI

/// A transfer queue the person can pause whole.
enum StorageQueueKind {
    case uploads
    /// Pinning and moving to This Mac: one queue.
    case downloads
    case outputs
}

/// The button that pauses a transfer queue whole, or resumes it while it is
/// paused, surfacing a failure in the window it sits in.
struct QueuePauseButton: View {
    enum Placement {
        case activity, footer
    }

    let queue: StorageQueueKind
    let paused: Bool
    let placement: Placement
    @Environment(Sync.self)
    private var sync
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        MacIconButton(
            symbol: paused ? "play.fill" : "pause.fill",
            label: pauseLabel,
            iconSize: placement == .activity ? .medium : .chip,
            look: placement == .activity ? .boxed(small: true) : .borderless,
            action: setPaused
        )
    }

    private var pauseLabel: LocalizedStringKey {
        if placement == .activity {
            return paused ? "Resume" : "Pause"
        }
        return switch (queue, paused) {
        case (.uploads, false): "Pause uploads"
        case (.uploads, true): "Resume uploads"
        case (.downloads, false): "Pause downloads"
        case (.downloads, true): "Resume downloads"
        case (.outputs, false): "Pause exports"
        case (.outputs, true): "Resume exports"
        }
    }

    /// Pause the whole queue, or resume it while paused.
    private func setPaused() {
        let paused = !paused
        let queue = queue
        Task {
            do {
                switch queue {
                case .uploads: try await sync.setSyncPaused(paused)
                case .downloads: try await downloads.setDownloadsPaused(paused)
                case .outputs: try await outputs.setOutputsPaused(paused)
                }
            }
            catch { uiStore.showError(error) }
        }
    }
}
