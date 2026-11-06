import Foundation

/// A clock a test moves by hand: a sleep ends when `advance(by:)` reaches its
/// deadline, never because time passed, and a cancelled sleep ends at once.
///
/// `untilSleeps(begun:)` returns once that many sleeps have begun, so a test
/// waits for a debounce to be armed instead of polling for it.
final class ManualClock: Clock, @unchecked Sendable {
    struct Instant: InstantProtocol {
        var offset: Duration

        func advanced(by duration: Duration) -> Instant {
            Instant(offset: offset + duration)
        }

        func duration(to other: Instant) -> Duration {
            other.offset - offset
        }

        static func < (lhs: Instant, rhs: Instant) -> Bool {
            lhs.offset < rhs.offset
        }
    }

    private struct Sleeper {
        let deadline: Instant
        let continuation: CheckedContinuation<Void, any Error>
    }

    private let lock = NSLock()
    private var current = Instant(offset: .zero)
    private var sleepers: [UInt64: Sleeper] = [:]
    private var nextId: UInt64 = 0
    private var begun = 0
    private var begunWaiters:
        [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []

    var now: Instant { lock.withLock { current } }
    var minimumResolution: Duration { .zero }

    /// Return once `count` sleeps have begun on this clock, ended or not.
    func untilSleeps(begun count: Int) async {
        await withCheckedContinuation { continuation in
            let ready = lock.withLock {
                if begun >= count { return true }
                begunWaiters.append((count, continuation))
                return false
            }
            if ready { continuation.resume() }
        }
    }

    func sleep(until deadline: Instant, tolerance: Duration?) async throws {
        let id = lock.withLock {
            nextId += 1
            return nextId
        }
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let outcome: Result<Void, any Error>? = lock.withLock {
                    if Task.isCancelled { return .failure(CancellationError()) }
                    if deadline <= current { return .success(()) }
                    sleepers[id] = Sleeper(
                        deadline: deadline,
                        continuation: continuation
                    )
                    return nil
                }
                if let outcome {
                    continuation.resume(with: outcome)
                }
                else {
                    sleepBegan()
                }
            }
        } onCancel: {
            let sleeper = lock.withLock { sleepers.removeValue(forKey: id) }
            sleeper?.continuation.resume(throwing: CancellationError())
        }
    }

    private func sleepBegan() {
        let ready: [CheckedContinuation<Void, Never>] = lock.withLock {
            begun += 1
            let due = begunWaiters.filter { $0.count <= begun }
            begunWaiters.removeAll { $0.count <= begun }
            return due.map(\.continuation)
        }
        for continuation in ready { continuation.resume() }
    }

    /// Move time forward, ending every sleep whose deadline it reaches.
    func advance(by duration: Duration) {
        let due: [Sleeper] = lock.withLock {
            current = current.advanced(by: duration)
            let ids = sleepers.filter { $0.value.deadline <= current }.keys
            return ids.compactMap { sleepers.removeValue(forKey: $0) }
        }
        for sleeper in due {
            sleeper.continuation.resume()
        }
    }
}
