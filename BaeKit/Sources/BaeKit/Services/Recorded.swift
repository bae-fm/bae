#if DEBUG
    import Foundation
    import Observation

    /// What a fixture or a test's fake records, changed from any thread, for
    /// a test or a screenshot that waits for it under observation tracking.
    /// A wait whose condition read the value reads the condition again after
    /// the next change.
    ///
    /// The value changes on whichever thread records it, and the waits read
    /// it on the main thread, so each change is told there, once it is made.
    /// Observation tracking starts watching what a condition read only once
    /// the whole condition has run, and tells of a change before the change
    /// is made: a change told on another thread can land between a wait's
    /// read and its tracking, or after the wait has read again, and the wait
    /// never wakes.
    public final class Recorded<Value>: Observable, @unchecked Sendable {
        private let registrar = ObservationRegistrar()
        private let lock = NSLock()
        private var stored: Value

        public init(_ value: Value) {
            stored = value
        }

        public var value: Value {
            registrar.access(self, keyPath: \.value)
            return lock.withLock { stored }
        }

        /// Changes the value under the lock, then tells of the change.
        @discardableResult
        public func change<Result>(
            _ change: (inout Value) throws -> Result
        ) rethrows -> Result {
            let result = try lock.withLock { try change(&stored) }
            if Thread.isMainThread {
                tell()
            }
            else {
                DispatchQueue.main.async { self.tell() }
            }
            return result
        }

        /// Tell every tracked read of the value that it changed.
        private func tell() {
            registrar.withMutation(of: self, keyPath: \.value) {}
        }
    }
#endif
