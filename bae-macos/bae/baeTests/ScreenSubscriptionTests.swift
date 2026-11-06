import BaeKit
import Foundation
import Observation
import Testing

@testable import bae

/// A pulled subscription answering with queued reads, each a value or a
/// failure, and parking a pull until the next one is queued. It counts the
/// pulls and the cancels it was asked for.
private final class QueuedReads: PulledSubscription, @unchecked Sendable {
    private struct ReadFailed: Error {}

    private let lock = NSLock()
    private var queued: [Result<Int, any Error>] = []
    private var waiter: CheckedContinuation<Int, any Error>?
    private var pullCount = 0
    private var cancelCount = 0

    var pulls: Int { lock.withLock { pullCount } }
    var cancels: Int { lock.withLock { cancelCount } }

    func answer(_ value: Int) {
        queue(.success(value))
    }

    func fail() {
        queue(.failure(ReadFailed()))
    }

    func next() async throws -> Int {
        try await withCheckedThrowingContinuation { continuation in
            let ready: Result<Int, any Error>? = lock.recording {
                pullCount += 1
                if queued.isEmpty {
                    waiter = continuation
                    return nil
                }
                return queued.removeFirst()
            }
            if let ready { continuation.resume(with: ready) }
        }
    }

    func cancel() async throws {
        let waiter = lock.recording {
            cancelCount += 1
            let waiter = self.waiter
            self.waiter = nil
            return waiter
        }
        waiter?.resume(throwing: BridgeError.Cancelled)
    }

    private func queue(_ read: Result<Int, any Error>) {
        let waiter = lock.withLock {
            let waiter = self.waiter
            self.waiter = nil
            if waiter == nil { queued.append(read) }
            return waiter
        }
        waiter?.resume(with: read)
    }
}

/// What a screen subscription handed on: each value, and how many failed
/// reads.
@MainActor
@Observable
private final class Handed {
    var values: [Int] = []
    var errors = 0
}

@Suite("Screen subscription")
struct ScreenSubscriptionTests {
    @Test("pulled values turn each value of their source, and end it")
    func pulledValuesTurnEachValueAndEndTheSource() async throws {
        let source = QueuedReads()
        let values = PulledValues(source) { "value \($0)" }
        source.answer(1)
        source.answer(2)

        #expect(try await values.next() == "value 1")
        #expect(try await values.next() == "value 2")
        try await values.cancel()
        #expect(source.cancels == 1)
    }

    @Test("a first-value read takes one value and ends the subscription")
    func firstValueTakesOneValueAndEnds() async throws {
        let source = QueuedReads()
        source.answer(7)
        source.answer(8)

        #expect(try await source.firstValue() == 7)
        #expect(source.pulls == 1)
        #expect(source.cancels == 1)
    }

    @Test("a first-value read that fails ends the subscription too")
    func failedFirstValueEnds() async {
        let source = QueuedReads()
        source.fail()

        await #expect(throws: (any Error).self) {
            try await source.firstValue()
        }
        #expect(source.cancels == 1)
    }

    @MainActor
    @Test("a failed read is handed on and the pulls go on")
    func pullsGoOnAfterAFailedRead() async throws {
        let source = QueuedReads()
        let handed = Handed()
        let subscription = ScreenSubscription(
            source,
            onValue: { handed.values.append($0) },
            onError: { _ in handed.errors += 1 }
        )
        source.answer(1)
        source.fail()
        source.answer(2)

        try await Wait.until { handed.values == [1, 2] }
        #expect(handed.errors == 1)

        subscription.stop()
        try await Wait.until { source.cancels >= 1 }
    }
}
