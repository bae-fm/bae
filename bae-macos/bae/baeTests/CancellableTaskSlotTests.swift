import BaeKit
import Foundation
import Testing

/// Exercises `CancellableTaskSlot`'s supersede and delivery semantics: a new
/// run cancels the one in flight, the detached-work envelope routes a value to
/// `onSuccess` and an error to `onError`, and a cancelled run swallows both.
@MainActor
@Suite("CancellableTaskSlot")
struct CancellableTaskSlotTests {

    @Test("a second run cancels the first, whose callbacks never fire")
    func replaceSupersedesInFlight() async {
        let slot = CancellableTaskSlot()
        let recorder = Recorder()
        let firstStarted = Latch()
        let firstGate = DispatchSemaphore(value: 0)
        let firstDone = Latch()

        // The first run parks in its detached work until released, so the
        // second run reliably supersedes it while it is genuinely in flight.
        slot.replace(
            "first",
            work: { () -> Int in
                defer { firstDone.fire() }
                firstStarted.fire()
                firstGate.wait()
                try Task.checkCancellation()
                return 1
            },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )
        await firstStarted.wait()

        slot.replace(
            "second",
            work: { 2 },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )

        // Release the first; superseded, it throws cancellation and lands
        // nothing, while the second delivers its value.
        firstGate.signal()
        await firstDone.wait()
        await recorder.awaitCallback()

        #expect(recorder.successes == [2])
        #expect(recorder.errors.isEmpty)
    }

    @Test("the envelope delivers the work's value to onSuccess")
    func envelopeDeliversValue() async {
        let slot = CancellableTaskSlot()
        let recorder = Recorder()

        slot.replace(
            "value",
            work: { 7 },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )
        await recorder.awaitCallback()

        #expect(recorder.successes == [7])
        #expect(recorder.errors.isEmpty)
    }

    @Test("the envelope routes a thrown error to onError")
    func envelopeRoutesError() async {
        struct WorkError: Error {}
        let slot = CancellableTaskSlot()
        let recorder = Recorder()

        slot.replace(
            "error",
            work: { () -> Int in throw WorkError() },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )
        await recorder.awaitCallback()

        #expect(recorder.successes.isEmpty)
        #expect(recorder.errors.count == 1)
        #expect(recorder.errors.first is WorkError)
    }

    @Test("cancelling a parked run swallows both callbacks")
    func cancelSwallowsCallbacks() async {
        let slot = CancellableTaskSlot()
        let recorder = Recorder()
        let started = Latch()
        let gate = DispatchSemaphore(value: 0)
        let done = Latch()

        slot.replace(
            "parked",
            work: { () -> Int in
                defer { done.fire() }
                started.fire()
                gate.wait()
                try Task.checkCancellation()
                return 1
            },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )
        await started.wait()

        slot.cancel()
        gate.signal()
        // Once the detached work has exited having thrown cancellation, neither
        // callback can fire: success needs a returned value, and the error arm
        // ignores `CancellationError`.
        await done.wait()

        #expect(recorder.successes.isEmpty)
        #expect(recorder.errors.isEmpty)
    }

    @Test("cancel with nothing in flight is a no-op and leaves the slot usable")
    func cancelWithNothingInFlight() async {
        let slot = CancellableTaskSlot()
        let recorder = Recorder()

        slot.cancel()

        slot.replace(
            "after-cancel",
            work: { 5 },
            onSuccess: { recorder.succeed($0) },
            onError: { recorder.fail($0) }
        )
        await recorder.awaitCallback()

        #expect(recorder.successes == [5])
    }

    // MARK: - Fixtures

    /// Collects the envelope callbacks on the main actor and lets a test await
    /// the first arrival, whichever order the delivery and the await happen in.
    @MainActor
    private final class Recorder {
        private(set) var successes: [Int] = []
        private(set) var errors: [Error] = []
        private var arrival: CheckedContinuation<Void, Never>?

        func succeed(_ value: Int) {
            successes.append(value)
            signalArrival()
        }

        func fail(_ error: Error) {
            errors.append(error)
            signalArrival()
        }

        func awaitCallback() async {
            if !successes.isEmpty || !errors.isEmpty { return }
            await withCheckedContinuation { arrival = $0 }
        }

        private func signalArrival() {
            arrival?.resume()
            arrival = nil
        }
    }

    /// A one-shot signal fireable from any thread — the detached work signals it
    /// off the main actor, and the test awaits it — resolving whichever order
    /// the fire and the wait happen in.
    private final class Latch: @unchecked Sendable {
        private let lock = NSLock()
        private var fired = false
        private var waiter: CheckedContinuation<Void, Never>?

        func fire() {
            lock.lock()
            let waiter = waiter
            self.waiter = nil
            fired = true
            lock.unlock()
            waiter?.resume()
        }

        func wait() async {
            await withCheckedContinuation { continuation in
                lock.lock()
                if fired {
                    lock.unlock()
                    continuation.resume()
                }
                else {
                    waiter = continuation
                    lock.unlock()
                }
            }
        }
    }
}
