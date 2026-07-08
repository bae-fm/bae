import BaeKit
import Foundation

/// Resolves album/track ids to flat track ids and applies a queue mutation,
/// surfacing resolve failures through the shared error path. Shared by the
/// now-playing-bar drops (`MainAppView`) and the album grid's bulk menu
/// (`LibraryView`) so both take the same resolve-then-apply route: album ids
/// expand to the primary release's tracks, track ids pass through.
@MainActor
final class QueueActions {
    private let library: Library
    private let queue: Queue
    private let uiStore: UiStore

    init(library: Library, queue: Queue, uiStore: UiStore) {
        self.library = library
        self.queue = queue
        self.uiStore = uiStore
    }

    /// Resolve `ids` and append the tracks to the end of the manual lane.
    func addToQueue(_ ids: [String]) {
        resolve(ids) { [queue] trackIds in queue.addToQueue(trackIds) }
    }

    /// Resolve `ids` and insert the tracks at the front of the manual lane.
    func addNext(_ ids: [String]) {
        resolve(ids) { [queue] trackIds in queue.addNext(trackIds) }
    }

    /// Resolve `ids` and insert the tracks at `index` in the manual lane.
    func insertInQueue(_ ids: [String], at index: Int) {
        resolve(ids) { [queue] trackIds in
            queue.insertInQueue(trackIds, UInt32(index))
        }
    }

    private func resolve(
        _ ids: [String],
        then apply: @escaping ([String]) -> Void
    ) {
        let library = library
        let uiStore = uiStore
        Task {
            do {
                let trackIds = try await library.resolveToTrackIds(ids)
                guard !trackIds.isEmpty else {
                    return
                }
                apply(trackIds)
            }
            catch {
                uiStore.showError(error.localizedDescription)
            }
        }
    }
}
