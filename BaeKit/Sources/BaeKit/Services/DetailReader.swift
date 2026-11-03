import Foundation
import os.log

private let logger = Logger.bae("DetailReader")

/// One value a detail read delivered: the id it was read for, and that
/// item's detail, or `nil` once no such item exists.
public struct KeyedDetailDelivery<Value: Sendable, Id: Hashable & Sendable>:
    Sendable
{
    public let id: Id?
    public let value: Value?

    public init(id: Id?, value: Value?) {
        self.id = id
        self.value = value
    }
}

/// A delivery of a read whose items are named by text ids.
public typealias DetailDelivery<Value: Sendable> = KeyedDetailDelivery<
    Value, String
>

/// A detail view's live read: the id it shows changes in place, and each
/// value names the id it answers.
public struct KeyedDetailQuery<Value: Sendable, Id: Hashable & Sendable>:
    Sendable
{
    public let setId: @Sendable (Id) throws -> Void
    public let values: PulledValues<KeyedDetailDelivery<Value, Id>>

    public init(
        setId: @escaping @Sendable (Id) throws -> Void,
        values: PulledValues<KeyedDetailDelivery<Value, Id>>
    ) {
        self.setId = setId
        self.values = values
    }
}

/// A read whose items are named by text ids.
public typealias DetailQuery<Value: Sendable> = KeyedDetailQuery<Value, String>

/// One detail view's read of the item it shows, open while the view shows
/// an item: showing another item moves the read in place instead of opening
/// another, and `close` ends it when the view stops showing one. Each value
/// is handed on only while it answers the item shown now.
@MainActor
public final class KeyedDetailReader<Value: Sendable, Id: Hashable & Sendable> {
    private let open: () -> KeyedDetailQuery<Value, Id>
    private let onValue: @MainActor (Id, Value?) -> Void
    private let onError: @MainActor (Id, any Error) -> Void
    private var query: KeyedDetailQuery<Value, Id>?
    private var deliveries:
        ScreenSubscription<PulledValues<KeyedDetailDelivery<Value, Id>>>?
    /// The item shown now, or `nil` before the first `show` and after
    /// `close`.
    public private(set) var id: Id?

    public init(
        open: @escaping () -> KeyedDetailQuery<Value, Id>,
        onValue: @escaping @MainActor (Id, Value?) -> Void,
        onError: @escaping @MainActor (Id, any Error) -> Void
    ) {
        self.open = open
        self.onValue = onValue
        self.onError = onError
    }

    /// Show `id`, opening the read on first use.
    public func show(_ id: Id) {
        guard id != self.id || query == nil else { return }
        self.id = id
        request(id)
    }

    /// Read the item shown now again on a fresh read, after one failed.
    public func retry() {
        guard let id else { return }
        closeRead()
        request(id)
    }

    /// Stop showing anything and end the read.
    public func close() {
        id = nil
        closeRead()
    }

    private func request(_ id: Id) {
        do {
            try (query ?? openRead()).setId(id)
        }
        catch {
            onError(id, error)
        }
    }

    private func openRead() -> KeyedDetailQuery<Value, Id> {
        let query = open()
        self.query = query
        deliveries = ScreenSubscription(
            query.values,
            onValue: { [weak self] delivery in
                guard let self, let id = delivery.id, id == self.id else {
                    return
                }
                self.onValue(id, delivery.value)
            },
            onError: { [weak self] error in self?.readFailed(error) }
        )
        return query
    }

    private func readFailed(_ error: any Error) {
        if let id {
            onError(id, error)
        }
        else {
            // Nothing is shown to report it on; the read goes on.
            logger.warning(
                "detail read failed while showing nothing: \(String(reflecting: error))"
            )
        }
    }

    private func closeRead() {
        deliveries?.stop()
        deliveries = nil
        query = nil
    }
}

/// A detail reader whose items are named by text ids.
public typealias DetailReader<Value: Sendable> = KeyedDetailReader<
    Value, String
>
