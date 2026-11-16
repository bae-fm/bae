import Observation

/// A value a live read returned, with the key the read asked for.
public struct Shown<Key, Value> {
    public let key: Key
    public let value: Value
}

/// One live read that a screen asks for by a key, such as a member id or a
/// place, while the screen shows it. Asking for another key replaces the
/// read, and `shown` keeps the last value until the new read returns its
/// first, so the screen compares `shown.key` when it must not draw another
/// key's value.
@MainActor
@Observable
public final class KeyedRead<Key: Equatable & Sendable, Value> {
    /// The value the read returned last, with the key it asked for; nil
    /// before the first read returns and after `hide`.
    public private(set) var shown: Shown<Key, Value>?
    /// The key the running read asks for, while one runs.
    @ObservationIgnored
    private var key: Key?
    @ObservationIgnored
    private var subscription: AnyObject?

    public init() {}

    /// Read `key` through `subscribe`, which starts the read, hands each
    /// value it returns to the closure it is given, and returns what keeps
    /// the read running. Asking again for the key that runs changes nothing.
    func show(
        _ key: Key,
        subscribe: (@escaping @MainActor @Sendable (Value) -> Void) throws ->
            AnyObject
    ) throws {
        guard self.key != key || subscription == nil else { return }
        subscription = nil
        self.key = key
        subscription = try subscribe { [weak self] value in
            // A value of a read that another key replaced is dropped.
            guard let self, self.key == key else { return }
            shown = Shown(key: key, value: value)
        }
    }

    /// End the read and forget what it returned, once no screen shows it.
    func hide() {
        subscription = nil
        key = nil
        shown = nil
    }
}
