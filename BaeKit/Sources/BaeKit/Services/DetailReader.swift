import Foundation

/// One value a detail read delivered: the id it was read for, and that
/// item's detail, or `nil` once no such item exists.
public struct DetailDelivery<Value: Sendable>: Sendable {
    public let id: String?
    public let value: Value?

    public init(id: String?, value: Value?) {
        self.id = id
        self.value = value
    }
}

/// A detail view's live read: the id it shows changes in place, and each
/// value names the id it answers.
public struct DetailQuery<Value: Sendable>: Sendable {
    public let setId: @Sendable (String?) throws -> Void
    public let next: @Sendable () async throws -> DetailDelivery<Value>
    public let cancel: @Sendable () async -> Void

    public init(
        setId: @escaping @Sendable (String?) throws -> Void,
        next: @escaping @Sendable () async throws -> DetailDelivery<Value>,
        cancel: @escaping @Sendable () async -> Void
    ) {
        self.setId = setId
        self.next = next
        self.cancel = cancel
    }

    /// A read over fixed details, for previews and tests: every id is
    /// answered with what `lookup` finds for it.
    public static func fixed(
        _ lookup: @escaping @Sendable (String) -> Value?
    ) -> DetailQuery<Value> {
        let requests = FixedRequests<String?>()
        return DetailQuery(
            setId: { requests.request($0) },
            next: {
                let id = try await requests.next()
                return DetailDelivery(id: id, value: id.flatMap(lookup))
            },
            cancel: { requests.close() }
        )
    }
}

/// One detail view's read of the item it shows, kept for as long as the view
/// is: showing another item moves the read in place instead of opening
/// another. Each value is handed on only while it answers the item shown now.
@MainActor
public final class DetailReader<Value: Sendable> {
    private let open: () -> DetailQuery<Value>
    private let onValue: @MainActor (String, Value?) -> Void
    private let onError: @MainActor (String, any Error) -> Void
    private var query: DetailQuery<Value>?
    private var deliveries: Task<Void, Never>?
    /// The item shown now, or `nil` before the first `show` and after
    /// `clear` or `close`.
    public private(set) var id: String?

    public init(
        open: @escaping () -> DetailQuery<Value>,
        onValue: @escaping @MainActor (String, Value?) -> Void,
        onError: @escaping @MainActor (String, any Error) -> Void
    ) {
        self.open = open
        self.onValue = onValue
        self.onError = onError
    }

    deinit {
        deliveries?.cancel()
        if let query {
            Task { await query.cancel() }
        }
    }

    /// Show `id`, opening the read on first use.
    public func show(_ id: String) {
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
        guard id != nil else { return }
        id = nil
        do {
            try query?.setId(nil)
        }
        catch {
            closeRead()
        }
    }

    /// Stop showing anything and end the read.
    public func close() {
        id = nil
        closeRead()
    }

    private func request(_ id: String) {
        do {
            try (query ?? openRead()).setId(id)
        }
        catch {
            onError(id, error)
        }
    }

    private func openRead() -> DetailQuery<Value> {
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
                    if let id = self.id { self.onError(id, error) }
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
