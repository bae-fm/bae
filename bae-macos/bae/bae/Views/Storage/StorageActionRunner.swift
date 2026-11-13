import AppKit
import BaeKit
import Observation
import SwiftUI

/// Runs Storage Manager row context-menu actions against a set of releases.
///
/// The Storage Manager rows render the `storageActions` the core already
/// computed per release (plus the cancel a transitioning release offers);
/// this type performs the chosen transition for every targeted release,
/// reusing the same `ReleaseEditor` / `Sync` service calls the album-detail
/// "Storage…" sheet uses. Errors surface through `UiStore` (shown by the
/// Storage Manager window's alert); subscribed rows and outbox values carry
/// transition results.
///
/// Moving to cloud storage is confirmed first, beside the stored pin choice,
/// so it stashes the targets in `pendingMoveToCloud` and the view presents
/// `MoveToCloudConfirmSheet`. Making
/// a release local opens an `NSOpenPanel` for the destination folder. The other
/// transitions run straight away. Unpin asks first while "Ask before
/// unpinning" is on, so it stashes the targets in `pendingUnpin` and the view
/// shows the alert (`unpinConfirmation`).
///
/// Every action acts only on the targets the list still holds when it starts —
/// after its sheet or panel, which the list can change under — so a release
/// that left the list is never acted on.
@MainActor
@Observable
final class StorageActionRunner {
    private let releaseEditor: ReleaseEditor
    private let sync: Sync
    private let downloads: Downloads
    private let outputs: Outputs
    private let configStore: ConfigStore
    private let uiStore: UiStore
    private let storageManagerStore: StorageManagerStore
    /// Opens Settings at the Save as pane, in front of a save panel.
    private let openSaveAsSettings: @MainActor () -> Void

    /// Releases awaiting the "Move to cloud" confirm sheet. Non-nil while
    /// the sheet is up; the view binds its presentation to this.
    var pendingMoveToCloud: [String]?

    /// Releases awaiting the Unpin alert. Non-nil while it is up; the view
    /// binds its presentation to this.
    var pendingUnpin: PendingUnpin?

    init(
        releaseEditor: ReleaseEditor,
        sync: Sync,
        downloads: Downloads,
        outputs: Outputs,
        configStore: ConfigStore,
        uiStore: UiStore,
        storageManagerStore: StorageManagerStore,
        openSaveAsSettings: @escaping @MainActor () -> Void
    ) {
        self.releaseEditor = releaseEditor
        self.sync = sync
        self.downloads = downloads
        self.outputs = outputs
        self.configStore = configStore
        self.uiStore = uiStore
        self.storageManagerStore = storageManagerStore
        self.openSaveAsSettings = openSaveAsSettings
    }

    /// The releases of `releaseIds` the list holds now, in their order.
    private func listed(_ releaseIds: [String]) -> [String] {
        let shown = storageManagerStore.shown(Set(releaseIds))
        return releaseIds.filter(shown.contains)
    }

    /// Export each release verbatim to a folder chosen once for the whole batch.
    /// No folder chosen → nothing enqueued. Each enqueue joins the in-memory
    /// output queue, which serializes the batch and reports progress via the
    /// Exporting pane.
    func export(releaseIds: [String]) {
        guard let targetDir = OutputTarget.resolveExportDir() else {
            return
        }
        let releaseIds = listed(releaseIds)
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
        Task {
            guard
                let target = await OutputTarget.resolveReleaseSave(
                    configStore: configStore,
                    openFormats: openSaveAsSettings
                )
            else {
                return
            }
            let releaseIds = listed(releaseIds)
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
    /// defers to the confirm sheet; moving to This Mac asks for a destination folder
    /// once and queues the whole batch on the download queue, each release in
    /// a folder of its own below it; pin enqueues the whole batch on the
    /// download queue; unpin asks first while the setting says to, then releases
    /// the whole batch's pins in one call.
    func run(_ action: BridgeReleaseStorageAction, releaseIds: [String]) {
        switch action {
        case .upload:
            pendingMoveToCloud = releaseIds
        case .detachAudio:
            detachAudio(releaseIds: releaseIds)
        case .pin:
            // Pinning routes through the download queue, which
            // serializes the batch and reports progress via the Downloads pane
            // and per-release transfer events — not an awaited per-release loop.
            let releaseIds = listed(releaseIds)
            guard !releaseIds.isEmpty else { return }
            Task {
                do { try await downloads.queuePins(releaseIds) }
                catch { uiStore.showError(error) }
            }
        case .unpin:
            let releaseIds = listed(releaseIds)
            guard !releaseIds.isEmpty else { return }
            if UnpinAsk.asks(
                askBeforeUnpinning: configStore.config.askBeforeUnpinning
            ) {
                pendingUnpin = PendingUnpin(
                    releaseIds: releaseIds,
                    title: releaseIds.count == 1
                        ? storageManagerStore.row(of: releaseIds[0])?.album
                            .title : nil
                )
            }
            else {
                unpin(releaseIds, steps: UnpinAsk.withoutAsking)
            }
        }
    }

    /// How the Unpin alert closed: the releases it asked about unpin as the
    /// answer says.
    func answerUnpin(_ pending: PendingUnpin, _ answer: UnpinAsk.Answer) {
        let releaseIds = listed(pending.releaseIds)
        let steps = UnpinAsk.steps(after: answer)
        guard !releaseIds.isEmpty, !steps.isEmpty else { return }
        unpin(releaseIds, steps: steps)
    }

    private func unpin(_ releaseIds: [String], steps: [UnpinAsk.Step]) {
        let downloads = downloads
        Task {
            do {
                try await UnpinAsk.run(
                    steps,
                    stopAsking: {
                        try await downloads.setAskBeforeUnpinning(false)
                    },
                    unpin: { try await downloads.unpinReleases(releaseIds) }
                )
            }
            catch { uiStore.showError(error) }
        }
    }

    /// Confirm callback for `MoveToCloudConfirmSheet`: admit each pending
    /// release independently.
    func confirmMoveToCloud() {
        guard let releaseIds = pendingMoveToCloud else {
            preconditionFailure("move-to-cloud confirmation has no releases")
        }
        pendingMoveToCloud = nil
        let listedIds = listed(releaseIds)
        guard !listedIds.isEmpty else { return }
        Task {
            do {
                try await releaseEditor.moveReleasesToCloud(listedIds)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }

    func cancelMoveToCloud() {
        pendingMoveToCloud = nil
    }

    /// Cancel each release's in-progress transition (pin / upload / move to This Mac),
    /// leaving it in its prior state — core dispatches to whichever is running.
    func cancelTransitions(releaseIds: [String]) {
        let releaseIds = listed(releaseIds)
        Task {
            for id in releaseIds {
                do {
                    try await sync.cancelReleaseTransition(id)
                }
                catch {
                    uiStore.showError(error)
                    return
                }
            }
        }
    }

    /// Prompt for the folder a local release's files move into. Returns
    /// the chosen directory, or nil when the user cancels the panel.
    static func promptDetachAudioDestination() -> URL? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Move here")
        guard panel.runModal() == .OK, let url = panel.url else {
            return nil
        }
        return url
    }

    private func detachAudio(releaseIds: [String]) {
        guard let newPath = Self.promptDetachAudioDestination() else {
            return
        }
        let releaseIds = listed(releaseIds)
        guard !releaseIds.isEmpty else { return }
        Task {
            do {
                try await releaseEditor.makeReleasesLocal(releaseIds, newPath)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}
