import BaeKit

struct LibraryShutdownFailure: Sendable {
    let displayedError: DisplayError?
    let diagnostic: String
}

enum LibraryShutdownResult: Sendable {
    case completed
    case failed(LibraryShutdownFailure)
}

/// Owns the one graceful shutdown allowed for an open library. Close and Quit
/// may both wait for it, but neither can replace it or turn its failure into
/// success.
@MainActor
final class LibraryShutdownCoordinator<Owner: AnyObject> {
    struct Attempt {
        let task: Task<LibraryShutdownResult, Never>
        let started: Bool
    }

    private struct PendingShutdown {
        let owner: Owner
        let task: Task<LibraryShutdownResult, Never>
    }

    private var pending: PendingShutdown?

    func begin(
        for owner: Owner,
        operation: @MainActor @escaping () async throws -> Void
    ) -> Attempt {
        if let pending {
            precondition(
                pending.owner === owner,
                "a different library cannot replace an active shutdown"
            )
            return Attempt(task: pending.task, started: false)
        }

        let task = Task { @MainActor in
            do {
                try await operation()
                return LibraryShutdownResult.completed
            }
            catch {
                return LibraryShutdownResult.failed(
                    LibraryShutdownFailure(
                        displayedError: DisplayError(error),
                        diagnostic: String(reflecting: error)
                    )
                )
            }
        }
        pending = PendingShutdown(owner: owner, task: task)
        return Attempt(task: task, started: true)
    }

    func hasPendingShutdown(for owner: Owner) -> Bool {
        pending?.owner === owner
    }

    func finish(for owner: Owner) {
        guard pending?.owner === owner else { return }
        pending = nil
    }
}
