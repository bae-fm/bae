import Combine
import Observation
import os.log

private let logger = Logger.bae("Projection")

final class ProjectionRegistration {
    private var cancellable: AnyCancellable?

    init(_ cancellable: AnyCancellable) {
        self.cancellable = cancellable
    }

    deinit {
        cancellable?.cancel()
    }
}

enum BridgeInvalidationDomain: Hashable {
    case albumList
    case album
    case release
    case composerList
    case composer
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
    var domain: BridgeInvalidationDomain {
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
final class ProjectionRegistry: Observable {
    private let invalidations = PassthroughSubject<BridgeInvalidation, Never>()

    @discardableResult
    func register<Value: Sendable>(
        _ projection: Projection<Value>
    ) -> ProjectionRegistration {
        register(
            domains: projection.domains,
            invalidate: projection.invalidate(for:)
        )
    }

    @discardableResult
    func registerList<Row>(
        _ list: PaginatedList<Row>,
        domain: BridgeInvalidationDomain
    ) -> ProjectionRegistration {
        register(
            domains: [domain],
            invalidate: { [list] _ in list.invalidate() }
        )
    }

    @discardableResult
    func register(
        domains: Set<BridgeInvalidationDomain>,
        invalidate: @escaping (BridgeInvalidation) -> Void
    ) -> ProjectionRegistration {
        ProjectionRegistration(
            invalidations
                .filter { domains.contains($0.domain) }
                .sink(receiveValue: invalidate)
        )
    }

    func invalidate(_ invalidation: BridgeInvalidation) {
        invalidations.send(invalidation)
    }
}

@MainActor
final class Projection<Value: Sendable> {
    let domains: Set<BridgeInvalidationDomain>
    private let query: @Sendable (BridgeInvalidation) async throws -> Value
    private let apply: @MainActor (Value) -> Void
    private let onError: @MainActor (DisplayError) -> Void

    private(set) var generation = 0
    private var requestedGeneration = 0
    private var reloadTask: Task<Void, Never>?

    convenience init(
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

    init(
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

    func matches(_ invalidation: BridgeInvalidation) -> Bool {
        domains.contains(invalidation.domain)
    }

    func invalidate(for invalidation: BridgeInvalidation) {
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
                onError(DisplayError(line: error.localizedDescription))
            }
        }
    }

}
