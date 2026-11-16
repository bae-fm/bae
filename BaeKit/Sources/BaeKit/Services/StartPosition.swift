import Foundation

/// Where a chain's first row sits in its list: the key of that row and the
/// number of rows before it, a live read of its own that follows the key.
/// The list's top has no key and sits at zero. Until the position read
/// returns a value for a key, StartPosition uses the position the chain gave
/// with the key.
@MainActor
final class StartPosition {
    private(set) var value = 0

    /// The key of the chain's first row; `nil` at the list's top and once
    /// `stop` has ended the read.
    var key: [BridgeSortValue]? { reader.id }

    private let position: @Sendable ([BridgeSortValue]) -> ListPositionQuery
    private let onChange: () -> Void
    private let onError: (any Error) -> Void
    private lazy var reader = KeyedDetailReader<UInt64, [BridgeSortValue]>(
        opening: { [position] key in Self.read(position(key)) },
        onValue: { [weak self] _, rowsBefore in
            guard let rowsBefore else {
                preconditionFailure(
                    "a list position read returns a row count for every key"
                )
            }
            self?.apply(Int(rowsBefore))
        },
        onError: { [onError] _, error in onError(error) }
    )

    init(
        position: @escaping @Sendable ([BridgeSortValue]) -> ListPositionQuery,
        onChange: @escaping () -> Void,
        onError: @escaping (any Error) -> Void
    ) {
        self.position = position
        self.onChange = onChange
        self.onError = onError
    }

    /// Follow `key` (the list's top when nil), using `position` as the
    /// number of rows before it until the position read returns.
    func follow(_ key: [BridgeSortValue]?, assuming position: Int) {
        guard let key else {
            value = 0
            reader.close()
            return
        }
        value = position
        reader.show(key)
    }

    func stop() {
        reader.close()
    }

    private func apply(_ rowsBefore: Int) {
        guard rowsBefore != value else { return }
        value = rowsBefore
        onChange()
    }

    /// Wraps the position query `query` in a keyed read whose values each
    /// name the key they count the rows before.
    private static func read(
        _ query: ListPositionQuery
    ) -> KeyedDetailQuery<UInt64, [BridgeSortValue]> {
        KeyedDetailQuery(
            setId: { try query.setKey($0) },
            values: PulledValues(query.values) { position in
                KeyedDetailDelivery(
                    id: position.key,
                    value: position.rowsBefore
                )
            }
        )
    }
}
