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
    case exportQueue
    case importCandidateList
    case importCandidate
    case watchedFolders
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
        case .exportQueue:
            return .exportQueue
        case .importCandidateList:
            return .importCandidateList
        case .importCandidate:
            return .importCandidate
        case .watchedFolders:
            return .watchedFolders
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
    private let onError: @MainActor (DisplayError) -> Void

    public private(set) var generation = 0
    private var requestedGeneration = 0
    private var reloadTask: Task<Void, Never>?

    public convenience init(
        domain: BridgeInvalidationDomain,
        query: @escaping @Sendable (BridgeInvalidation) async throws -> Value,
        apply: @escaping @MainActor (Value) -> Void,
        onError: @escaping @MainActor (DisplayError) -> Void
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
        onError: @escaping @MainActor (DisplayError) -> Void
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

    public func invalidate(for invalidation: BridgeInvalidation) {
        guard matches(invalidation) else {
            return
        }
        reloadTask?.cancel()
        requestedGeneration &+= 1
        let generation = requestedGeneration
        reloadTask = Task { [weak self] in
            guard let self else {
                return
            }
            do {
                let value = try await query(invalidation)
                guard self.requestedGeneration == generation else {
                    return
                }
                apply(value)
                self.generation = generation
            }
            catch is CancellationError {
            }
            catch {
                logger.error(
                    "Projection refresh failed: \(error.localizedDescription)"
                )
                onError(DisplayError(error))
            }
        }
    }

}
