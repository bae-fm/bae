import Foundation

extension Task where Success == Never, Failure == Never {
    /// Suspends until the current task is cancelled, for work that holds
    /// something for as long as the task that asked for it runs.
    public static func untilCancelled() async {
        let gate = CancellationGate()
        await withTaskCancellationHandler {
            await withCheckedContinuation { gate.wait($0) }
        } onCancel: {
            gate.open()
        }
    }
}

/// Resumes the one waiter once opened, whichever of the two comes first.
private final class CancellationGate: @unchecked Sendable {
    private let lock = NSLock()
    private var opened = false
    private var waiter: CheckedContinuation<Void, Never>?

    func wait(_ continuation: CheckedContinuation<Void, Never>) {
        let resumeNow = lock.withLock {
            if opened { return true }
            waiter = continuation
            return false
        }
        if resumeNow {
            continuation.resume()
        }
    }

    func open() {
        let waiter = lock.withLock {
            opened = true
            let waiter = self.waiter
            self.waiter = nil
            return waiter
        }
        waiter?.resume()
    }
}
