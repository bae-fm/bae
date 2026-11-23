import Observation

/// A heart choice that the view shows until the library finishes writing it.
@MainActor
@Observable
final class PendingHeart {
    private(set) var choice: Bool?

    func choose(
        _ hearted: Bool,
        write: @escaping (Bool) async -> Void
    ) {
        guard choice == nil else { return }
        choice = hearted
        // Keep the choice even if the person leaves the album before the
        // library finishes writing it.
        Task {
            defer { choice = nil }
            await write(hearted)
        }
    }
}
