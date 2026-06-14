import Foundation

/// Namespace for off-main-thread helpers. `DetachedWork.run` is the
/// shared idiom for shipping a synchronous `@Sendable` bridge call to a
/// detached worker while propagating caller-side cancellation (`.task`
/// disappearing, a button-action `Task` being replaced) through to it.
enum DetachedWork {
    static func run<T: Sendable>(
        _ body: @escaping @Sendable () throws -> T
    ) async throws -> T {
        let detached = Task.detached { try body() }
        return try await withTaskCancellationHandler {
            try await detached.value
        } onCancel: {
            detached.cancel()
        }
    }
}
