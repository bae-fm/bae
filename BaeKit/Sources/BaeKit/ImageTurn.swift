import Foundation

extension BridgePictureTurn {
    /// Where the turn stands among the three: a higher rank goes first.
    var rank: Int {
        switch self {
        case .prefetch: 0
        case .visible: 1
        case .focused: 2
        }
    }
}

/// How soon one request wants its image. Whoever asks for the image keeps
/// this, and changes it while the request waits, as what the person looks at
/// changes.
public final class ImageTurn: @unchecked Sendable {
    private let lock = NSLock()
    /// Guarded by `lock`.
    private var asked: BridgePictureTurn
    /// What each load this request waits on runs when the turn changes, by
    /// the request's place in that load. Guarded by `lock`.
    private var followers: [ObjectIdentifier: @Sendable () -> Void] = [:]

    public init(_ asked: BridgePictureTurn = .visible) {
        self.asked = asked
    }

    /// The turn asked now.
    var current: BridgePictureTurn {
        lock.withLock { asked }
    }

    public func set(_ turn: BridgePictureTurn) {
        let told: [@Sendable () -> Void] = lock.withLock {
            guard asked != turn else { return [] }
            asked = turn
            return Array(followers.values)
        }
        for tell in told {
            tell()
        }
    }

    func follow(
        by follower: ObjectIdentifier,
        _ changed: @escaping @Sendable () -> Void
    ) {
        lock.withLock { followers[follower] = changed }
    }

    func unfollow(by follower: ObjectIdentifier) {
        lock.withLock { followers[follower] = nil }
    }
}

/// The turn of one load: the highest any request waiting on it asks. The
/// load's read in core and its decode both wait in this turn.
final class LoadTurn: @unchecked Sendable {
    /// What core's read of the load's picture watches.
    let signal: BridgePictureTurnSignal
    private let lock = NSLock()
    /// What each waiting request asks, by its place in the load. Guarded by
    /// `lock`.
    private var asks: [ObjectIdentifier: ImageTurn] = [:]

    init(_ first: BridgePictureTurn) {
        signal = BridgePictureTurnSignal(turn: first)
    }

    var current: BridgePictureTurn { signal.turn() }

    func add(_ waiter: ObjectIdentifier, asking turn: ImageTurn) {
        lock.withLock { asks[waiter] = turn }
        turn.follow(by: waiter) { [weak self] in self?.publish() }
        publish()
    }

    func remove(_ waiter: ObjectIdentifier) {
        let turn = lock.withLock { asks.removeValue(forKey: waiter) }
        turn?.unfollow(by: waiter)
        publish()
    }

    /// Sets the signal to the highest turn asked. A load no request waits on
    /// keeps the turn it had.
    private func publish() {
        // The lock is held across the set, so that two changes reach the
        // signal in the order they were made.
        lock.withLock {
            let highest = asks.values.map(\.current)
                .max { $0.rank < $1.rank }
            if let highest {
                signal.set(turn: highest)
            }
        }
    }
}
