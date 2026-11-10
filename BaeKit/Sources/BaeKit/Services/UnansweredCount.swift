#if DEBUG
    import Foundation
    import Observation

    /// How many things a fixture or a store was asked for and has not yet
    /// handed back, for a test or a screenshot that waits, under observation
    /// tracking, until none are left.
    ///
    /// The count changes on whichever thread an answer lands, and the waits
    /// read it on the main thread, so each change is told there, once it is
    /// made. Observation tracking reads a value before it starts tracking
    /// it, and tells of a change before the change is made: a change told on
    /// another thread can land between a wait's read and its tracking, or
    /// after the wait has read again, and the wait never wakes.
    public final class UnansweredCount: Observable, @unchecked Sendable {
        private let registrar = ObservationRegistrar()
        private let lock = NSLock()
        private var count = 0

        public init() {}

        /// The things asked for and not yet handed back.
        public var value: Int {
            registrar.access(self, keyPath: \.value)
            return lock.withLock { count }
        }

        func change(by delta: Int) {
            guard delta != 0 else { return }
            lock.withLock { count += delta }
            if Thread.isMainThread {
                tell()
            }
            else {
                DispatchQueue.main.async { self.tell() }
            }
        }

        /// Tell every tracked read of the count that it changed.
        private func tell() {
            registrar.withMutation(of: self, keyPath: \.value) {}
        }
    }
#endif
