import Foundation

extension HangGuard {
    /// [`duration`] for the XCTest waits that take seconds. The duration
    /// stays under the minute XCTest gives each test (`bae-macos/test.sh`),
    /// so a hung wait fails with its own message first.
    static var seconds: TimeInterval {
        TimeInterval(duration.components.seconds)
    }
}
