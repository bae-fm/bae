import Foundation

/// The clock the shots read: a fixed moment, so a countdown or an "ago" draws
/// the same words whenever the shot is taken. `ShotStage` sets it as the
/// environment's `clockNow`, and a scene builds each moment it shows from it.
enum ShotClock {
    /// The moment every shot is drawn at.
    static let now = Date(timeIntervalSince1970: 1_800_000_000)

    /// The moment a countdown that has `seconds` left ends.
    static func moment(secondsLeft seconds: TimeInterval) -> Date {
        now.addingTimeInterval(seconds)
    }

    /// The moment `seconds` before the shots' clock.
    static func moment(secondsAgo seconds: TimeInterval) -> Date {
        now.addingTimeInterval(-seconds)
    }
}
