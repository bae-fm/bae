import SwiftUI

extension EnvironmentValues {
    /// The clock typing pauses are measured on; a test sets one it moves by
    /// hand.
    @Entry
    public var debounceClock: any Clock<Duration> = ContinuousClock()
}
