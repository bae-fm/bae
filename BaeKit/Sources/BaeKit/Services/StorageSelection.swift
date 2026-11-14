import Foundation

/// The rows of the releases selected in the Storage Manager list that the
/// list holds, read live: the selected ids change in place, and each value
/// answers the ids last set.
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
