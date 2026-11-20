import BaeKit
import Foundation
import Observation

/// The list History shows: everyone's plays or one person's, read live
/// through a `HistoryList` while the History tab shows, from `show` to
/// `cancel`. A new choice of whose plays opens another list, and the store
/// keeps the list it had until the new one answers, so the side pane keeps
/// drawing the last plays until the new ones arrive.
@MainActor
@Observable
final class HistoryStore {
    /// The list on screen, `nil` until the first one answers.
    private(set) var list: HistoryList?

    @ObservationIgnored
    private let library: Library
    @ObservationIgnored
    private let onError: (any Error) -> Void
    /// A list opened for a new choice that has not answered yet.
    private struct Opening {
        let list: HistoryList
        let task: Task<Void, Never>
    }

    @ObservationIgnored
    private var opening: Opening?

    init(library: Library, onError: @escaping (any Error) -> Void) {
        self.library = library
        self.onError = onError
    }

    /// Show the plays of the person with member id `person`, or everyone's
    /// for `nil`. The view calls it when the choice changes.
    func show(person: String?) {
        opening?.task.cancel()
        opening?.list.cancel()
        let newList = HistoryList(
            queries: library.historyList(person),
            onError: onError
        )
        let task = Task { [weak self] in
            await newList.loadInitial()
            guard !Task.isCancelled, let self else { return }
            self.list?.cancel()
            self.list = newList
            self.opening = nil
        }
        opening = Opening(list: newList, task: task)
    }

    /// End every read the store holds, once History no longer shows.
    func cancel() {
        opening?.task.cancel()
        opening?.list.cancel()
        opening = nil
        list?.cancel()
        list = nil
    }
}
