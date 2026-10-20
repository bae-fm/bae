import Observation

/// Waiting in a test for observable state that changes later, such as a
/// screen landing once a bridge call returns.
enum Observed {
    /// Return once `condition` holds, checking it again each time state it
    /// read changes: the wait ends on the change itself, however long the
    /// work behind it runs. A change that never comes holds the test until
    /// its time limit fails it.
    @MainActor
    static func until(_ condition: @MainActor () -> Bool) async {
        while !condition() {
            await withCheckedContinuation { continuation in
                withObservationTracking {
                    _ = condition()
                } onChange: {
                    continuation.resume()
                }
            }
        }
    }
}
