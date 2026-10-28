/// The latest value of each key a fast keyed stream names, such as how far
/// each file in flight has got, for native views that draw one key's value
/// straight from the stream. Nothing here is observed by SwiftUI: a view
/// registers for its key and is told each value as it lands, so a change
/// redraws only the views that draw that key and runs no SwiftUI update.
@MainActor
public final class KeyedValues<Value> {
    /// A view's registration for one key, which it hands back to stop.
    public struct Observation: Hashable, Sendable {
        let key: String
        let id: Int
    }

    private var values: [String: Value] = [:]
    /// Each observed key's observers, by registration; a key leaves the map
    /// with its last observer.
    private var observers: [String: [Int: (Value?) -> Void]] = [:]
    private var nextId = 0

    public init() {}

    /// The key's value now; `nil` while the stream names none for it.
    public func value(for key: String) -> Value? {
        values[key]
    }

    /// The stream named `key` with `value`.
    public func set(_ value: Value, for key: String) {
        values[key] = value
        tell(key, value)
    }

    /// The stream stopped naming `key`.
    public func remove(_ key: String) {
        guard values.removeValue(forKey: key) != nil else { return }
        tell(key, nil)
    }

    /// The stream named every key it holds: `all`, and none other.
    public func reset(_ all: [String: Value]) {
        let gone = values.keys.filter { all[$0] == nil }
        values = all
        for key in gone {
            tell(key, nil)
        }
        for (key, value) in all {
            tell(key, value)
        }
    }

    /// Tell `apply` the key's value now, then each value it takes, `nil`
    /// while the stream names none, until `stopObserving`.
    public func observe(
        _ key: String,
        _ apply: @escaping (Value?) -> Void
    ) -> Observation {
        let observation = Observation(key: key, id: nextId)
        nextId += 1
        observers[key, default: [:]][observation.id] = apply
        apply(values[key])
        return observation
    }

    public func stopObserving(_ observation: Observation) {
        observers[observation.key]?.removeValue(forKey: observation.id)
        if observers[observation.key]?.isEmpty == true {
            observers.removeValue(forKey: observation.key)
        }
    }

    private func tell(_ key: String, _ value: Value?) {
        guard let keyed = observers[key] else { return }
        for apply in keyed.values {
            apply(value)
        }
    }
}

/// One key of a keyed stream, which a view hands to the native view that
/// draws it.
public struct KeyedValueSource<Value> {
    public let values: KeyedValues<Value>
    public let key: String

    public init(values: KeyedValues<Value>, key: String) {
        self.values = values
        self.key = key
    }

    /// Whether `other` names the same key of the same stream.
    public func isSame(as other: Self) -> Bool {
        values === other.values && key == other.key
    }
}

/// A native view's following of one key of a keyed stream: it follows a
/// new key when handed one, and stops when the view goes.
@MainActor
public final class KeyedValueFollowing<Value> {
    private var source: KeyedValueSource<Value>?
    private var observation: KeyedValues<Value>.Observation?

    public init() {}

    /// Tell `apply` the source's value now and each one after; a source for
    /// the key already followed changes nothing.
    public func follow(
        _ source: KeyedValueSource<Value>,
        _ apply: @escaping (Value?) -> Void
    ) {
        if let current = self.source, current.isSame(as: source) { return }
        stop()
        self.source = source
        observation = source.values.observe(source.key, apply)
    }

    public func stop() {
        if let source, let observation {
            source.values.stopObserving(observation)
        }
        source = nil
        observation = nil
    }
}
