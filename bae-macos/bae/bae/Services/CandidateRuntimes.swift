import BaeKit
import Observation

/// What every candidate has in flight, as core's runtime subscription last
/// said: a run's identify state and a running import's step, by candidate
/// key. A view reads its own key, and redraws as that key changes or as keys
/// come and go; how far an import has got is the import percents' stream,
/// which no view observes.
@MainActor
@Observable
final class CandidateRuntimes {
    /// One key's runtime, observed on its own so a change to it redraws only
    /// the views reading that key.
    @Observable
    final class Value {
        fileprivate(set) var snapshot: BridgeCandidateRuntimeSnapshot

        fileprivate init(_ snapshot: BridgeCandidateRuntimeSnapshot) {
            self.snapshot = snapshot
        }
    }

    /// The keys with something in flight; it changes only as one starts or
    /// ends.
    private var keys: Set<String>
    @ObservationIgnored
    private var values: [String: Value]

    /// Nothing in flight until the subscription says, or, for a preview, the
    /// runtimes `held`.
    init(holding held: [String: BridgeCandidateRuntimeSnapshot] = [:]) {
        keys = Set(held.keys)
        values = held.mapValues(Value.init)
    }

    /// What is in flight for `key`; `nil` while nothing is.
    func runtime(forKey key: String) -> BridgeCandidateRuntimeSnapshot? {
        guard keys.contains(key) else { return nil }
        return values[key]?.snapshot
    }

    /// Apply one batch the subscription delivered, in order.
    func apply(_ changes: [BridgeCandidateRuntimeChange]) {
        for change in changes {
            switch change {
            case .updated(let key, let runtime):
                set(runtime, for: key)
            case .removed(let key):
                values.removeValue(forKey: key)
                keys.remove(key)
            case .reset(let runtimes):
                let next = Dictionary(
                    runtimes.map { ($0.key, $0.runtime) },
                    uniquingKeysWith: { _, last in last }
                )
                for key in keys.subtracting(next.keys) {
                    values.removeValue(forKey: key)
                }
                for (key, runtime) in next {
                    set(runtime, for: key)
                }
                if keys != Set(next.keys) {
                    keys = Set(next.keys)
                }
            }
        }
    }

    private func set(_ runtime: BridgeCandidateRuntimeSnapshot, for key: String)
    {
        if let value = values[key] {
            value.snapshot = runtime
        }
        else {
            values[key] = Value(runtime)
            keys.insert(key)
        }
    }
}
