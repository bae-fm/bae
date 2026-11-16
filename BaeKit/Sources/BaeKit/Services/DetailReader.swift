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
    private let open: (Id) throws -> KeyedDetailQuery<Value, Id>
    private let onValue: @MainActor (Id, Value?) -> Void
    private let onError: @MainActor (Id, any Error) -> Void
    /// The read open now, from the first `show` until `close` or a `retry`
    /// that opens another.
    private var opened: OpenRead?
    /// The item shown now, or `nil` before the first `show` and after
    /// `close`.
    public private(set) var id: Id?

    /// A reader whose `open` opens the read of the id it is given.
    public init(
        opening open: @escaping (Id) throws -> KeyedDetailQuery<Value, Id>,
        onValue: @escaping @MainActor (Id, Value?) -> Void,
        onError: @escaping @MainActor (Id, any Error) -> Void
    ) {
        self.open = open
        self.onValue = onValue
        self.onError = onError
    }

    /// A reader whose `open` opens a read of no id, which the reader then
    /// moves to the id shown.
    public convenience init(
        open: @escaping () -> KeyedDetailQuery<Value, Id>,
        onValue: @escaping @MainActor (Id, Value?) -> Void,
        onError: @escaping @MainActor (Id, any Error) -> Void
    ) {
        self.init(
            opening: { id in
                let query = open()
                try query.setId(id)
                return query
            },
            onValue: onValue,
            onError: onError
        )
    }

    /// Show `id`, opening the read on first use.
    public func show(_ id: Id) {
        guard id != self.id || opened == nil else { return }
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
            if let opened {
                try opened.query.setId(id)
            }
            else {
                try openRead(id)
            }
        }
        catch {
            onError(id, error)
        }
    }

    private func openRead(_ id: Id) throws {
        let query = try open(id)
        opened = OpenRead(
            query: query,
            deliveries: ScreenSubscription(
                query.values,
                onValue: { [weak self] delivery in
                    guard let self, let id = delivery.id, id == self.id else {
                        return
                    }
                    self.onValue(id, delivery.value)
                },
                onError: { [weak self] error in self?.readFailed(error) }
            )
        )
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
        opened?.deliveries.stop()
        opened = nil
    }

    /// A read the reader opened, with the subscription that hands on its
    /// values.
    private struct OpenRead {
        let query: KeyedDetailQuery<Value, Id>
        let deliveries:
            ScreenSubscription<PulledValues<KeyedDetailDelivery<Value, Id>>>
    }
}

/// A detail reader whose items are named by text ids.
public typealias DetailReader<Value: Sendable> = KeyedDetailReader<
    Value, String
>
