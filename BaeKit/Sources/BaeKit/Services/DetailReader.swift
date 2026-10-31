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
    public let setId: @Sendable (Id?) throws -> Void
    public let next: @Sendable () async throws -> KeyedDetailDelivery<Value, Id>
    public let cancel: @Sendable () async -> Void

    public init(
        setId: @escaping @Sendable (Id?) throws -> Void,
        next:
            @escaping @Sendable () async throws -> KeyedDetailDelivery<
                Value, Id
            >,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setId = setId
        self.next = next
        self.cancel = cancel
    }
}

/// A read whose items are named by text ids.
public typealias DetailQuery<Value: Sendable> = KeyedDetailQuery<Value, String>

/// One detail view's read of the item it shows, kept for as long as the view
/// is: showing another item moves the read in place instead of opening
/// another. Each value is handed on only while it answers the item shown now.
@MainActor
public final class KeyedDetailReader<Value: Sendable, Id: Hashable & Sendable> {
    private let open: () -> KeyedDetailQuery<Value, Id>
    private let onValue: @MainActor (Id, Value?) -> Void
    private let onError: @MainActor (Id, any Error) -> Void
    /// The text a log line names an id by.
    private let idText: (Id) -> String
    private var query: KeyedDetailQuery<Value, Id>?
    private var deliveries: Task<Void, Never>?
    /// The item shown now, or `nil` before the first `show` and after
    /// `clear` or `close`.
    public private(set) var id: Id?

    public init(
        open: @escaping () -> KeyedDetailQuery<Value, Id>,
        onValue: @escaping @MainActor (Id, Value?) -> Void,
        onError: @escaping @MainActor (Id, any Error) -> Void,
        idText: @escaping (Id) -> String
    ) {
        self.open = open
        self.onValue = onValue
        self.onError = onError
        self.idText = idText
    }

    deinit {
        deliveries?.cancel()
        if let query {
            Task { await query.cancel() }
        }
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

    /// Stop showing anything, keeping the read open for the next `show`: it
    /// reads nothing until then.
    public func clear() {
        guard let shown = id else { return }
        id = nil
        do {
            try query?.setId(nil)
        }
        catch {
            // A read that can no longer be pointed anywhere has ended; the
            // next `show` opens another.
            logger.warning(
                "detail read showing \(idText(shown)) could not be cleared, so it is closed: \(String(reflecting: error))"
            )
            closeRead()
        }
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
        deliveries = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let delivery = try await query.next()
                    guard let self else { return }
                    guard let id = delivery.id, id == self.id else { continue }
                    self.onValue(id, delivery.value)
                }
                catch BridgeError.Cancelled {
                    return
                }
                catch is CancellationError {
                    return
                }
                catch {
                    guard let self else { return }
                    if let id = self.id {
                        self.onError(id, error)
                    }
                    else {
                        // Nothing is shown to report it on; the read goes on.
                        logger.warning(
                            "detail read failed while showing nothing: \(String(reflecting: error))"
                        )
                    }
                }
            }
        }
        return query
    }

    private func closeRead() {
        deliveries?.cancel()
        deliveries = nil
        if let query {
            self.query = nil
            Task { await query.cancel() }
        }
    }
}

/// A detail reader whose items are named by text ids.
public typealias DetailReader<Value: Sendable> = KeyedDetailReader<
    Value, String
>

extension KeyedDetailReader where Id == String {
    /// A reader whose ids are their own text.
    public convenience init(
        open: @escaping () -> KeyedDetailQuery<Value, Id>,
        onValue: @escaping @MainActor (Id, Value?) -> Void,
        onError: @escaping @MainActor (Id, any Error) -> Void
    ) {
        self.init(
            open: open,
            onValue: onValue,
            onError: onError,
            idText: { $0 }
        )
    }
}
