import Foundation

/// Waiting in a test for something that happens later: a value landing in a
/// store, a recorder receiving a call, a hosted view finishing its layout.
///
/// Every wait holds out until what it waits for is true or a deadline in
/// wall-clock time passes, and a deadline that passes throws. Counting a
/// fixed number of run-loop turns, or sleeping a fixed time, was a guess at
/// how busy the machine was: a loaded full run needed more than the guess
/// allowed, and a wait that then fell through quietly left the test to fail
/// on an assertion about something that had simply not arrived yet — or to
/// pass on a check made before the thing it was about had happened.
enum Wait {
    /// What a wait was given did not happen in time.
    struct TimedOut: LocalizedError, CustomStringConvertible {
        let after: Duration
        let file: StaticString
        let line: UInt

        var description: String {
            "the wait at \(file):\(line) was still waiting after \(after)"
        }

        var errorDescription: String? { description }
    }

    /// How long any one wait holds out. Far past anything a passing test
    /// needs, so reaching it means the thing waited for is not coming.
    static let deadline: Duration = .seconds(10)

    /// How long a value must stay the same to count as steady: several
    /// run-loop passes, each of which may lay a view out or run the
    /// main-actor work a view started.
    static let steadiness: Duration = .milliseconds(40)

    /// Return once `condition` holds, checking it between short sleeps that
    /// hand the main actor and its run loop to whatever the test started.
    @MainActor
    static func until(
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: @MainActor () async throws -> Bool
    ) async throws {
        let clock = ContinuousClock()
        let end = clock.now.advanced(by: timeout)
        while try await !condition() {
            guard clock.now < end else {
                throw TimedOut(after: timeout, file: file, line: line)
            }
            try await Task.sleep(for: .milliseconds(1))
        }
    }

    /// Return once `value` has read the same for `steadiness`: a hosted view
    /// whose frames stopped moving, a list whose geometry came to rest.
    ///
    /// A view that has not started moving yet also reads the same, so when
    /// the step before the wait must change the value — a scroll — the
    /// caller passes what it read before that step as `changedFrom`, and the
    /// wait holds out for a reading that differs from it first.
    @MainActor
    static func untilSteady<Value: Equatable>(
        changedFrom before: Value? = nil,
        timeout: Duration = deadline,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ value: @MainActor () async throws -> Value
    ) async throws {
        let clock = ContinuousClock()
        let end = clock.now.advanced(by: timeout)
        var last = try await value()
        var changed = true
        if let before { changed = before != last }
        var steadySince = clock.now
        while true {
            try await Task.sleep(for: .milliseconds(1))
            let now = try await value()
            if now != last {
                last = now
                if let before, !changed { changed = before != now }
                steadySince = clock.now
            }
            else if changed, clock.now - steadySince >= steadiness {
                return
            }
            guard clock.now < end else {
                throw TimedOut(after: timeout, file: file, line: line)
            }
        }
    }
}
