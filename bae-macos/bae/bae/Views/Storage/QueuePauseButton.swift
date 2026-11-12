import BaeKit
import SwiftUI

/// A transfer queue the person can pause whole.
enum StorageQueueKind {
    case uploads
    /// Pinning and making local: one queue.
    case downloads
    case outputs
}

/// The button that pauses a transfer queue whole, or resumes it while it is
/// paused, surfacing a failure in the window it sits in. The caller sets its
/// look with a button style.
struct QueuePauseButton: View {
    let queue: StorageQueueKind
    let paused: Bool
    @Environment(Sync.self)
    private var sync
    @Environment(Downloads.self)
    private var downloads
    @Environment(Outputs.self)
    private var outputs
    @Environment(UiStore.self)
    private var uiStore

    var body: some View {
        Button(action: setPaused) {
            Image(systemName: paused ? "play.fill" : "pause.fill")
                .themeIcon(.chip)
        }
        .accessibilityLabel(pauseLabel)
        .help(pauseLabel)
    }

    private var pauseLabel: Text {
        switch (queue, paused) {
        case (.uploads, false): Text("Pause uploads")
        case (.uploads, true): Text("Resume uploads")
        case (.downloads, false): Text("Pause downloads")
        case (.downloads, true): Text("Resume downloads")
        case (.outputs, false): Text("Pause exports")
        case (.outputs, true): Text("Resume exports")
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
