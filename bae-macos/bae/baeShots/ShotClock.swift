import Foundation

/// The clock the shots' countdowns read: a fixed moment, so a countdown draws
/// the same line whenever the shot is taken.
enum ShotClock {
    /// The moment a shot's countdowns count from.
    static let now = Date(timeIntervalSince1970: 1_800_000_000)

    /// The moment a countdown that has `seconds` left ends.
    static func moment(secondsLeft seconds: TimeInterval) -> Date {
        now.addingTimeInterval(seconds)
    }
}
