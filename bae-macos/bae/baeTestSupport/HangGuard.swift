import Foundation

/// How long a test waits for something it knows is coming before it calls
/// the hang a failure: the one limit of every wait in the unit, UI and shot
/// tests.
///
/// Every wait it bounds ends on its own event, as soon as the event comes,
/// so it paces nothing and sits far above what any step takes on a loaded
/// machine. It stays under the minute XCTest gives each test
/// (`bae-macos/test.sh`), so a hung wait fails with its own message first.
enum HangGuard {
    static let duration: Duration = .seconds(45)

    /// [`duration`] for the XCTest waits that take seconds.
    static var seconds: TimeInterval {
        TimeInterval(duration.components.seconds)
    }
}
