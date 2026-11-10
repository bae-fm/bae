import Foundation
import Observation
import Testing

@testable import BaeKit

/// A wait for a count reads it on the main thread, while the count changes
/// on whichever thread an answer lands.
@MainActor
struct UnansweredCountTests {
    @Test(
        "a change made off the main thread is told on the main thread, once it is made"
    )
    func changeOffTheMainThreadIsToldOnItOnceMade() async throws {
        let count = UnansweredCount()
        count.change(by: 1)
        let tell = Tell()
        withObservationTracking {
            _ = count.value
        } onChange: {
            tell.record(onMainThread: Thread.isMainThread, value: count.value)
        }

        await Task.detached { count.change(by: -1) }.value
        try await Wait.until { tell.told != nil }

        let told = try #require(tell.told)
        #expect(told.onMainThread)
        #expect(told.value == 0)
    }
}

/// Where and with what value a tracked read of the count was told it
/// changed.
private final class Tell: @unchecked Sendable {
    struct Told {
        let onMainThread: Bool
        let value: Int
    }

    private let lock = NSLock()
    private var recorded: Told?

    var told: Told? { lock.withLock { recorded } }

    func record(onMainThread: Bool, value: Int) {
        lock.recording {
            recorded = Told(onMainThread: onMainThread, value: value)
        }
    }
}
