import AppKit
import Observation
import SwiftUI

/// Runs Storage Manager row context-menu actions against a set of releases.
///
/// The Storage Manager rows render the `storageActions` the core already
/// computed per release (plus the Uploading tab's outbox cancel); this type
/// performs the chosen transition for every targeted release, reusing the same
/// `ReleaseEditor` / `Sync` service calls the album-detail "Storage…" sheet
/// uses. Errors surface through `UiStore` (shown by the Storage Manager
/// window's alert); the rows themselves refresh reactively when core re-emits
/// `ReleaseUpdated` / `AlbumUpdated` / `OutboxChanged`.
///
/// `manage` (move into library) needs the pin / delete-source choice, so it
/// stashes the targets in `pendingManage` and the view presents
/// `ManageConfirmSheet`. `unmanage` opens an `NSOpenPanel` for the destination
/// folder. The other transitions run straight away.
@MainActor
@Observable
final class StorageActionRunner {
    private let releaseEditor: ReleaseEditor
    private let sync: Sync
    private let downloads: Downloads
    private let uiStore: UiStore

    /// Releases awaiting the "Move into library" confirm sheet. Non-nil while
    /// the sheet is up; the view binds its presentation to this.
    var pendingManage: [String]?

    init(
        releaseEditor: ReleaseEditor,
        sync: Sync,
        downloads: Downloads,
        uiStore: UiStore
    ) {
        self.releaseEditor = releaseEditor
        self.sync = sync
        self.downloads = downloads
        self.uiStore = uiStore
    }

    /// Run `action` against every release in `releaseIds`. `manage` defers to
    /// the confirm sheet; `unmanage` asks for a destination folder once and
    /// moves each release into it; `pin` enqueues the whole batch on the
    /// download queue; `unpin` runs directly.
    func run(_ action: BridgeReleaseStorageAction, releaseIds: [String]) {
        switch action {
        case .manage:
            pendingManage = releaseIds
        case .unmanage:
            unmanage(releaseIds: releaseIds)
        case .pin:
            // Pinning routes through the in-memory download queue, which
            // serializes the batch and reports progress via the Downloads pane
            // and per-release transfer events — not an awaited per-release loop.
            downloads.queuePins(releaseIds)
        case .unpin:
            runEach(releaseIds, "remove local copy", releaseEditor.unpinRelease)
        }
    }

    /// Confirm callback for `ManageConfirmSheet`: move each pending release
    /// into the library with the chosen pin / delete-source options.
    func confirmManage(pin: Bool, deleteSource: Bool) {
        let releaseIds = pendingManage ?? []
        pendingManage = nil
        runEach(releaseIds, "move into library") { releaseId in
            try await self.releaseEditor.manageRelease(
                releaseId,
                pin,
                deleteSource
            )
        }
    }

    func cancelManage() {
        pendingManage = nil
    }

    /// Cancel every queued upload belonging to any of `releaseIds`. The
    /// caller resolves the outbox op ids from the current snapshot.
    func cancelUploads(opIds: [Int64]) {
        for id in opIds {
            do {
                try sync.cancelOutboxItem(id)
            }
            catch {
                uiStore.showError(
                    "Failed to cancel upload: \(error.localizedDescription)"
                )
                return
            }
        }
    }

    private func unmanage(releaseIds: [String]) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Move Here"

        guard panel.runModal() == .OK, let url = panel.url else {
            return
        }
        let newPath = url.path(percentEncoded: false)
        runEach(releaseIds, "move out of library") { releaseId in
            try await self.releaseEditor.unmanageRelease(releaseId, newPath)
        }
    }

    /// Run an async per-release transition for each id, surfacing the first
    /// failure. `verb` names the action for the error message ("failed to pin
    /// for offline: …"). Each bridge call descends into the cloud future chain
    /// on a runtime worker; progress renders from the per-release
    /// `ReleaseTransferProgress` events, not here.
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
                    "Failed to \(verb): \(error.localizedDescription)"
                )
            }
        }
    }
}
