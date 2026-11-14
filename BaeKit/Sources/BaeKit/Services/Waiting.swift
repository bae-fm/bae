import Foundation

/// Tasks waiting for something to land, each under a key, let go when what
/// they wait for has landed, when their own task is cancelled, or when the
/// owner gives up.
@MainActor
final class Waiting {
    private struct Waiter {
        let key: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    private var waiters: [UUID: Waiter] = [:]

    /// Wait under `key`, unless `done` already. The wait ends with
    /// `release(where:)` naming the key, `releaseAll`, or the task's
    /// cancellation, and never starts for a task already cancelled.
    func wait(key: Int = 0, unless done: Bool) async {
        guard !done else { return }
        let id = UUID()
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                guard !Task.isCancelled else {
                    continuation.resume()
                    return
                }
                waiters[id] = Waiter(key: key, continuation: continuation)
            }
        } onCancel: {
            Task { @MainActor [weak self] in
                self?.waiters.removeValue(forKey: id)?.continuation.resume()
            }
        }
    }

    /// Let go of the waiters whose key `done` answers for.
    func release(where done: (Int) -> Bool) {
        for (id, waiter) in waiters where done(waiter.key) {
            waiters.removeValue(forKey: id)
            waiter.continuation.resume()
        }
    }

    func releaseAll() {
        release { _ in true }
    }
}
