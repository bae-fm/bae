import Foundation

/// Waiting suspends tasks, each under a key. The owner resumes a task when
/// what it waits for has arrived or when the owner gives up, and a cancelled
/// task resumes itself.
@MainActor
final class Waiting {
    private struct Waiter {
        let key: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    private var waiters: [UUID: Waiter] = [:]

    /// Suspend the task under `key`, unless `done` is already true. The wait
    /// ends with
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

    /// Resume the waiters whose key makes `done` return true.
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
