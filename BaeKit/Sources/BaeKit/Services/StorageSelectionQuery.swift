import Foundation

/// A live query reads the rows of the selected releases that the Storage
/// Manager list holds. The caller changes the selected ids on the same
/// query, and each value holds the rows for the ids the caller set last.
public struct StorageSelectionQuery: Sendable {
    public let setSelected: @Sendable ([String]) throws -> Void
    public let values: PulledValues<BridgeStorageSelection>

    public init(
        setSelected: @escaping @Sendable ([String]) throws -> Void,
        values: PulledValues<BridgeStorageSelection>
    ) {
        self.setSelected = setSelected
        self.values = values
    }

    init(_ subscription: StorageSelectionSubscription) {
        self.init(
            setSelected: { try subscription.setSelected(selected: $0) },
            values: PulledValues(subscription)
        )
    }
}
