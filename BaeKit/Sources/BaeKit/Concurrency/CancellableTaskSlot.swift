import Foundation
import OSLog

private let logger = Logger.bae("CancellableTaskSlot")

/// A single-occupancy slot for a supersedable main-actor task: starting a
/// new run cancels the one still in flight, so at most one is live and a
/// stale result never lands after a newer request.
@MainActor
public final class CancellableTaskSlot {
    private var task: Task<Void, Never>?

    public init() {}

    /// Cancel the run in flight and start `operation` in its place.
    public func replace(_ operation: @escaping @MainActor () async -> Void) {
        task?.cancel()
        task = Task { @MainActor in await operation() }
    }

    /// Cancel the run in flight, ship `work` to a detached worker via
    /// `DetachedWork.run`, and deliver on the main actor: the value to
    /// `onSuccess`, any error to `onError`. Cancellation (a newer `replace`,
    /// or `cancel`) is swallowed with a debug log — the superseding run owns
    /// whatever surface the callbacks would have touched.
    public func replace<T: Sendable>(
        _ label: String,
        work: @escaping @Sendable () throws -> T,
        onSuccess: @escaping @MainActor (T) -> Void,
        onError: @escaping @MainActor (Error) -> Void
    ) {
        replace {
            do {
                let value = try await DetachedWork.run(work)
                onSuccess(value)
            }
            catch is CancellationError {
                logger.debug("\(label) cancelled")
            }
            catch {
                onError(error)
            }
        }
    }

    /// Cancel the run in flight, await `work`, and deliver its result on the
    /// main actor. The operation itself owns the runtime on which it executes;
    /// this slot owns only replacement and UI delivery.
    public func replace<T: Sendable>(
        _ label: String,
        work: @escaping @MainActor @Sendable () async throws -> T,
        onSuccess: @escaping @MainActor (T) -> Void,
        onError: @escaping @MainActor (Error) -> Void
    ) {
        replace {
            do {
                let value = try await work()
                try Task.checkCancellation()
                onSuccess(value)
            }
            catch is CancellationError {
                logger.debug("\(label) cancelled")
            }
            catch {
                onError(error)
            }
        }
    }

    /// Cancel the run in flight without starting a new one.
    public func cancel() {
        task?.cancel()
    }
}
