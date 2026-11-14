import Foundation

/// Where a chain's first row sits in its list: the key of that row and the
/// count of the rows before it, a live read of its own that follows the key.
/// The list's top has no key and sits at zero. Until a count answers for a
/// key, the position it was given stands in.
@MainActor
final class StartPosition {
    private(set) var key: [BridgeSortValue]?
    private(set) var value = 0

    private let position: @Sendable ([BridgeSortValue]) -> ListPositionQuery
    private let onChange: () -> Void
    private let onError: (any Error) -> Void
    private var query: ListPositionQuery?
    private var deliveries:
        ScreenSubscription<PulledValues<BridgeListPosition>>?

    init(
        position: @escaping @Sendable ([BridgeSortValue]) -> ListPositionQuery,
        onChange: @escaping () -> Void,
        onError: @escaping (any Error) -> Void
    ) {
        self.position = position
        self.onChange = onChange
        self.onError = onError
    }

    /// Follow `key` (the list's top when nil), standing `position` in for its
    /// count until the count answers.
    func follow(_ key: [BridgeSortValue]?, assuming position: Int) {
        self.key = key
        value = key == nil ? 0 : position
        guard let key else {
            stop()
            return
        }
        if let query {
            do {
                try query.setKey(key)
            }
            catch {
                onError(error)
            }
            return
        }
        let query = self.position(key)
        self.query = query
        deliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] position in self?.apply(position) },
            onError: { [weak self] error in self?.onError(error) }
        )
    }

    func stop() {
        deliveries?.stop()
        deliveries = nil
        query = nil
    }

    private func apply(_ position: BridgeListPosition) {
        // A value for a key the chain has since left draws nothing.
        guard position.key == key else { return }
        let rowsBefore = Int(position.rowsBefore)
        guard rowsBefore != value else { return }
        value = rowsBefore
        onChange()
    }
}
