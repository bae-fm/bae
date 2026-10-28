import Foundation
import os

/// The latest value of each key a fast keyed stream names, such as how far
/// each file in flight has got. The stream's callbacks write here on the
/// thread they arrive on, with no hop to the main thread; the native view
/// drawing a key reads its latest value once per display frame while the key
/// is active, so the screen takes values at its own refresh rate however
/// fast they come. Only a key starting or stopping reaches the main thread,
/// to start or stop the views drawing it. Nothing here is observed by
/// SwiftUI.
public final class LatestByKey<Value: Sendable & Equatable>: Sendable {
    /// A view's watch of whether one key is active, which it hands back to
    /// stop.
    public struct Watch: Hashable, Sendable {
        let key: String
        let id: Int
    }

    private struct Watchers {
        var byKey: [String: [Int: @MainActor @Sendable () -> Void]] = [:]
        var nextId = 0
    }

    private let values = OSAllocatedUnfairLock<[String: Value]>(
        initialState: [:]
    )
    private let watchers = OSAllocatedUnfairLock(initialState: Watchers())

    public init() {}

    /// The key's latest value; `nil` while the stream names none for it.
    public func latest(for key: String) -> Value? {
        values.withLock { $0[key] }
    }

    /// The stream named `key` with `value`.
    public func set(_ value: Value, for key: String) {
        let started = values.withLock { values in
            values.updateValue(value, forKey: key) == nil
        }
        if started { tell(key) }
    }

    /// The stream stopped naming `key`.
    public func remove(_ key: String) {
        let stopped = values.withLock { values in
            values.removeValue(forKey: key) != nil
        }
        if stopped { tell(key) }
    }

    /// The stream named every key it holds: `all`, and none other.
    public func reset(_ all: [String: Value]) {
        let changed = values.withLock { values in
            let keys = Set(values.keys).symmetricDifference(all.keys)
            values = all
            return keys
        }
        for key in changed {
            tell(key)
        }
    }

    /// Tell `changed` on the main thread each time `key` starts or stops
    /// being named, until `stopWatching`; it reads `latest(for:)` for which.
    public func watch(
        _ key: String,
        _ changed: @escaping @MainActor @Sendable () -> Void
    ) -> Watch {
        watchers.withLock { watchers in
            let watch = Watch(key: key, id: watchers.nextId)
            watchers.nextId += 1
            watchers.byKey[key, default: [:]][watch.id] = changed
            return watch
        }
    }

    public func stopWatching(_ watch: Watch) {
        watchers.withLock { watchers in
            watchers.byKey[watch.key]?.removeValue(forKey: watch.id)
            if watchers.byKey[watch.key]?.isEmpty == true {
                watchers.byKey.removeValue(forKey: watch.key)
            }
        }
    }

    /// Hop to the main thread once for a key that started or stopped, for
    /// the views watching it.
    private func tell(_ key: String) {
        let watching = watchers.withLock {
            Array(($0.byKey[key] ?? [:]).values)
        }
        guard !watching.isEmpty else { return }
        Task { @MainActor in
            for changed in watching {
                changed()
            }
        }
    }
}

/// One key of a fast keyed stream, which a view hands to the native view
/// that draws it.
public struct LatestSource<Value: Sendable & Equatable>: Sendable {
    public let store: LatestByKey<Value>
    public let key: String

    public init(store: LatestByKey<Value>, key: String) {
        self.store = store
        self.key = key
    }

    public var latest: Value? { store.latest(for: key) }

    /// Whether `other` names the same key of the same stream.
    public func isSame(as other: Self) -> Bool {
        store === other.store && key == other.key
    }
}

/// The latest value of a fast stream with one value, such as the files being
/// sent together: a one-key `LatestByKey`, holding the value while it is
/// active and none while it isn't.
public final class LatestOne<Value: Sendable & Equatable>: Sendable {
    private static var key: String { "" }
    private let store = LatestByKey<Value>()

    public init() {}

    /// The stream's value now, active.
    public func set(_ value: Value) {
        store.set(value, for: Self.key)
    }

    /// The stream went idle.
    public func clear() {
        store.remove(Self.key)
    }

    public var latest: Value? { store.latest(for: Self.key) }

    /// The stream, for the native view that draws it.
    public var source: LatestSource<Value> {
        LatestSource(store: store, key: Self.key)
    }
}
