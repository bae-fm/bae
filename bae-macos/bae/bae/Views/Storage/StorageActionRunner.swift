import AppKit
import BaeKit
import Observation
import SwiftUI

/// Runs Storage Manager row context-menu actions against a set of releases.
///
/// The Storage Manager rows render the `storageActions` the core already
/// computed per release (plus the Uploading tab's outbox cancel); this type
/// performs the chosen transition for every targeted release, reusing the same
/// `ReleaseEditor` / `Sync` service calls the album-detail "Storage…" sheet
/// uses. Errors surface through `UiStore` (shown by the Storage Manager
/// window's alert); subscribed rows and outbox values carry transition results.
///
/// Moving to cloud storage needs the pin choice, so it stashes the targets in
/// `pendingMoveToCloud` and the view presents `MoveToCloudConfirmSheet`. Making
/// a release local opens an `NSOpenPanel` for the destination folder. The other
/// transitions run straight away.
@MainActor
@Observable
final class StorageActionRunner {
    private let releaseEditor: ReleaseEditor
    private let sync: Sync
    private let downloads: Downloads
    private let outputs: Outputs
    private let configStore: ConfigStore
    private let uiStore: UiStore

    /// Releases awaiting the "Move to Cloud" confirm sheet. Non-nil while
    /// the sheet is up; the view binds its presentation to this.
    var pendingMoveToCloud: [String]?

    init(
        releaseEditor: ReleaseEditor,
        sync: Sync,
        downloads: Downloads,
        outputs: Outputs,
        configStore: ConfigStore,
        uiStore: UiStore
    ) {
        self.releaseEditor = releaseEditor
        self.sync = sync
        self.downloads = downloads
        self.outputs = outputs
        self.configStore = configStore
        self.uiStore = uiStore
    }

    /// Export each release verbatim to a folder chosen once for the whole batch.
    /// No folder chosen → nothing enqueued. Each enqueue joins the in-memory
    /// output queue, which serializes the batch and reports progress via the
    /// Exporting pane.
    func export(releaseIds: [String]) {
        guard let targetDir = OutputTarget.resolveExportDir() else {
            return
        }
        Task {
            for releaseId in releaseIds {
                do {
                    try await outputs.enqueueExport(releaseId, targetDir)
                }
                catch {
                    uiStore.showError(error)
                    return
                }
            }
        }
    }

    /// Save each release under one preset + folder chosen once for the whole
    /// batch. No folder chosen → nothing enqueued. Joins the same output queue as
    /// export.
    func saveAs(releaseIds: [String]) {
        guard
            let target = OutputTarget.resolveReleaseSave(
                config: configStore.config
            )
        else {
            return
        }
        Task {
            for releaseId in releaseIds {
                do {
                    try await outputs.enqueueReleaseSave(
                        releaseId,
                        target.targetDir,
                        target.presetId
                    )
                }
                catch {
                    uiStore.showError(error)
                    return
                }
            }
        }
    }

    /// Run `action` against every release in `releaseIds`. Moving to cloud
    /// defers to the confirm sheet; making local asks for a destination folder
    /// once and moves each release into it; pin enqueues the whole batch on the
    /// download queue; unpin runs directly.
    func run(_ action: BridgeReleaseStorageAction, releaseIds: [String]) {
        switch action {
        case .makeRemote:
            pendingMoveToCloud = releaseIds
        case .makeLocal:
            makeLocal(releaseIds: releaseIds)
        case .pin:
            // Pinning routes through the in-memory download queue, which
            // serializes the batch and reports progress via the Downloads pane
            // and per-release transfer events — not an awaited per-release loop.
            Task { try await downloads.queuePins(releaseIds) }
        case .unpin:
            runEach(
                releaseIds,
                String(localized: "unpin"),
                downloads.unpinRelease
            )
        }
    }

    /// Confirm callback for `MoveToCloudConfirmSheet`: move each pending
    /// release to cloud storage, pinning it when `pin` is set.
    func confirmMoveToCloud(pin: Bool) {
        let releaseIds = pendingMoveToCloud ?? []
        pendingMoveToCloud = nil
        runEach(releaseIds, String(localized: "move to cloud")) {
            releaseId in
            try await self.releaseEditor.moveReleaseToCloud(releaseId, pin)
        }
    }

    func cancelMoveToCloud() {
        pendingMoveToCloud = nil
    }

    /// Cancel each release's in-progress transition (pin / upload / make local),
    /// leaving it in its prior state — core dispatches to whichever is running.
    func cancelTransitions(releaseIds: [String]) {
        Task {
            for id in releaseIds {
                do {
                    try await sync.cancelReleaseTransition(id)
                }
                catch {
                    uiStore.showError(
                        String(
                            localized:
                                "Failed to cancel: \(error.displayLine)"
                        )
                    )
                    return
                }
            }
        }
    }

    /// Prompt for the folder a local release's files move into. Returns
    /// the chosen directory path, or nil when the user cancels the panel.
    static func promptMakeLocalDestination() -> String? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Move Here")
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        return url.path(percentEncoded: false)
    }

    private func makeLocal(releaseIds: [String]) {
        guard let newPath = Self.promptMakeLocalDestination() else {
            return
        }
        runEach(releaseIds, String(localized: "make local")) {
            releaseId in
            try await self.releaseEditor.makeReleaseLocal(releaseId, newPath)
        }
    }

    /// Run an async per-release transition for each id, surfacing the first
    /// failure. `verb` names the action for the error message ("failed to pin:
    /// …"). Each bridge call descends into the cloud future chain
    /// on a runtime worker; progress renders from subscribed release values.
    private func runEach(
        _ releaseIds: [String],
        _ verb: String,
        _ transition: @escaping @Sendable (String) async throws -> Void
    ) {
        Task {
            do {
                for releaseId in releaseIds {
                    try await transition(releaseId)
                }
            }
            catch {
                uiStore.showError(
                    String(
                        localized:
                            "Failed to \(verb): \(error.displayLine)"
                    )
                )
            }
        }
    }
}
