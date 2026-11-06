import Foundation
import Testing

@testable import BaeKit

@Suite("A waiting join's next check")
struct BootstrapStatusViewTests {
    private let now = Date(timeIntervalSinceReferenceDate: 1_000)

    @Test(
        "the seconds left round up and never go below zero",
        arguments: [
            (300.0, 300),
            (299.4, 300),
            (0.2, 1),
            (0.0, 0),
            (-1.0, 0),
        ]
    )
    func secondsLeftRoundUp(untilDue: Double, expected: Int) {
        let due = now.addingTimeInterval(untilDue)
        #expect(NextCheckLine.secondsLeft(until: due, at: now) == expected)
    }
}
