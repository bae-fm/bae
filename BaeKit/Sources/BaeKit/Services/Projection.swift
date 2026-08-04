import Combine
import Observation
import os.log

private let logger = Logger.bae("Projection")

public final class ProjectionRegistration {
    private var cancellable: AnyCancellable?

    public init(_ cancellable: AnyCancellable) {
        self.cancellable = cancellable
    }

    deinit {
        cancellable?.cancel()
    }
}

public enum BridgeInvalidationDomain: Hashable {
    case albumList
    case album
    case release
    case composerList
    case composer
    case artistList
    case queue
    case config
    case syncStatus
    case outbox
    case downloadQueue
    case outputQueue
    case importCandidateList
    case importCandidate
    case watchedFolders
    case castDevices
}

extension BridgeInvalidation {
    public var domain: BridgeInvalidationDomain {
        switch self {
        case .albumList:
            return .albumList
        case .album:
            return .album
        case .release:
            return .release
        case .composerList:
            return .composerList
        case .composer:
            return .composer
        case .artistList:
            return .artistList
        case .queue:
            return .queue
        case .config:
            return .config
        case .syncStatus:
            return .syncStatus
        case .outbox:
            return .outbox
        case .downloadQueue:
            return .downloadQueue
        case .outputQueue:
            return .outputQueue
        case .importCandidateList:
            return .importCandidateList
        case .importCandidate:
            return .importCandidate
        case .watchedFolders:
            return .watchedFolders
        case .castDevices:
            return .castDevices
        }
    }
}

@MainActor
public final class ProjectionRegistry: Observable {
    private let invalidations = PassthroughSubject<BridgeInvalidation, Never>()

    public init() {}

    @discardableResult
    public func register<Value: Sendable>(
        _ projection: Projection<Value>
    ) -> ProjectionRegistration {
        register(
            domains: projection.domains,
            invalidate: projection.invalidate(for:)
        )
    }

    @discardableResult
    public func registerList<Row>(
        _ list: PaginatedList<Row>,
        domain: BridgeInvalidationDomain
    ) -> ProjectionRegistration {
        register(
            domains: [domain],
            invalidate: { [list] _ in list.invalidate() }
        )
    }

    @discardableResult
    public func register(
        domains: Set<BridgeInvalidationDomain>,
        invalidate: @escaping (BridgeInvalidation) -> Void
    ) -> ProjectionRegistration {
        ProjectionRegistration(
            invalidations
                .filter { domains.contains($0.domain) }
                .sink(receiveValue: invalidate)
        )
    }

    public func invalidate(_ invalidation: BridgeInvalidation) {
        invalidations.send(invalidation)
    }
}

@MainActor
public final class Projection<Value: Sendable> {
    public let domains: Set<BridgeInvalidationDomain>
    private let query: @Sendable (BridgeInvalidation) async throws -> Value
    private let apply: @MainActor (Value) -> Void
    /// The failure sink. It takes the error, not a rendered `DisplayError`:
    /// whether a failure is worth showing at all is core's answer (a cancellation
    /// is not), and `showError` is the one place that drops it.
    private let onError: @MainActor (any Error) -> Void

    /// How many reads have landed. Monotone, so a caller can tell a store that
    /// has been filled from one that never has.
    public private(set) var generation = 0
    private var running = false
    /// The invalidation that arrived while a read was out, if any. One slot
    /// rather than a queue: a re-read answers every invalidation that arrived
    /// before it started, so only the newest is worth keeping.
    private var pending: BridgeInvalidation?

    public convenience init(
        domain: BridgeInvalidationDomain,
        query: @escaping @Sendable (BridgeInvalidation) async throws -> Value,
        apply: @escaping @MainActor (Value) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        self.init(
            domains: [domain],
            query: query,
            apply: apply,
            onError: onError
        )
    }

    public init(
        domains: Set<BridgeInvalidationDomain>,
        query: @escaping @Sendable (BridgeInvalidation) async throws -> Value,
        apply: @escaping @MainActor (Value) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        precondition(!domains.isEmpty)
        self.domains = domains
        self.query = query
        self.apply = apply
        self.onError = onError
    }

    public func matches(_ invalidation: BridgeInvalidation) -> Bool {
        domains.contains(invalidation.domain)
    }

    /// Read again for `invalidation`, or — with a read already out — queue one
    /// re-read behind it.
    ///
    /// Coalescing rather than cancel-and-restart: a bridge query is
    /// cancellable, so a domain invalidated faster than its query completes
    /// would be starved, killing every read before it lands and freezing the
    /// store for as long as the events keep coming (import progress against
    /// the triage queue is exactly that shape). Waiting costs one extra read
    /// per burst and guarantees the store moves.
    public func invalidate(for invalidation: BridgeInvalidation) {
        guard matches(invalidation) else {
            return
        }
        guard !running else {
            pending = invalidation
            return
        }
        read(invalidation)
    }

    private func read(_ invalidation: BridgeInvalidation) {
        running = true
        Task { [weak self] in
            guard let self else {
                return
            }
            do {
                let value = try await query(invalidation)
                apply(value)
                generation &+= 1
            }
            catch is CancellationError {
            }
            catch {
                logger.error(
                    "Projection refresh failed: \(error.localizedDescription)"
                )
                onError(error)
            }
            running = false
            if let next = pending {
                pending = nil
                read(next)
            }
        }
    }

}
