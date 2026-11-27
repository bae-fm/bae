#if DEBUG
    /// How many things a fixture or a store was asked for and has not yet
    /// handed back, for a test or a screenshot that waits, under observation
    /// tracking, until none are left.
    public final class UnansweredCount: Sendable {
        private let count = Recorded(0)

        public init() {}

        /// The things asked for and not yet handed back.
        public var value: Int { count.value }

        func change(by delta: Int) {
            guard delta != 0 else { return }
            count.change { $0 += delta }
        }
    }
#endif
