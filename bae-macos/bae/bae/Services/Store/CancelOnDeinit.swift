import Foundation

/// Reference-type wrapper that cancels its task when the wrapper is dropped.
/// Lets us hang an in-flight `Task` off a value-type `Candidate`: when the
/// candidate is removed from the store (or replaced by a fresh one), the
/// wrapper deinits and the task is cancelled. uniffi forwards Swift task
/// cancellation to the Rust future, which drops the in-flight HTTP request.
///
/// `Equatable` by identity so the synthesised `Candidate` equality still
/// works without comparing task contents. `Sendable` because the only stored
/// field is a `Task` (itself `Sendable`) held in an immutable `let`.
final class CancelOnDeinit: Equatable, Sendable {
    let task: Task<Void, Never>

    init(_ task: Task<Void, Never>) {
        self.task = task
    }

    deinit {
        task.cancel()
    }

    static func == (lhs: CancelOnDeinit, rhs: CancelOnDeinit) -> Bool {
        lhs === rhs
    }
}
