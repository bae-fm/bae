import Foundation
import Observation
import os.log

private let logger = Logger.bae("MembershipStore")

/// One join request awaiting an admin, with the details read off it once.
public struct PendingJoinRequest {
    public let request: BridgeJoinRequest
    public let info: BridgeJoinRequestInfo
}

/// The store's members joined with their profiles, and the join requests an
/// admin can approve. Membership and profiles are subscribed; each change of
/// either rereads core's joined list of people.
@MainActor
@Observable
public final class MembershipStore {
    /// The current members with their profiles, or nil before the first read.
    public private(set) var members: [BridgePerson]?
    public private(set) var selfIsAdmin = false
    /// This device's person's profile, nil until they have one.
    public private(set) var selfProfile: BridgePersonProfile?
    public private(set) var requests: [PendingJoinRequest] = []
    /// The last failed read; cleared by the next successful one.
    public private(set) var error: DisplayError?

    /// Starts the membership, profile and join-request subscriptions.
    private let subscribe: @MainActor (MembershipStore) -> [LiveSubscription]
    private let readPeopleOperation: @Sendable () async throws -> [BridgePerson]
    @ObservationIgnored
    private var subscriptions: [LiveSubscription] = []
    @ObservationIgnored
    private let peopleRead = CancellableTaskSlot()

    public convenience init(handle: AppHandle) {
        self.init(
            subscribe: { store in
                [
                    handle.subscribeMembers(
                        callback: MembershipSink(
                            apply: { [weak store] membership in
                                store?.selfIsAdmin = membership.selfIsAdmin
                                store?.readPeople()
                            },
                            fail: { [weak store] error in store?.fail(error) }
                        )
                    ),
                    handle.subscribeProfiles(
                        callback: ProfilesSink(
                            apply: { [weak store] in store?.readPeople() },
                            fail: { [weak store] error in store?.fail(error) }
                        )
                    ),
                    handle.subscribeJoinRequests(
                        callback: JoinRequestsSink { [weak store] requests in
                            store?.requests = requests.map {
                                PendingJoinRequest(request: $0, info: $0.info())
                            }
                        }
                    ),
                ]
            },
            readPeople: { try await handle.people() }
        )
    }

    private init(
        subscribe: @escaping @MainActor (MembershipStore) -> [LiveSubscription],
        readPeople: @escaping @Sendable () async throws -> [BridgePerson]
    ) {
        self.subscribe = subscribe
        readPeopleOperation = readPeople
    }

    #if DEBUG
        /// A store showing `people` from the start, with no subscriptions.
        public static func stub(
            people: [BridgePerson],
            selfIsAdmin: Bool
        ) -> MembershipStore {
            let store = MembershipStore(
                subscribe: { _ in [] },
                readPeople: { people }
            )
            store.selfIsAdmin = selfIsAdmin
            store.readPeople()
            return store
        }
    #endif

    public func start() {
        precondition(subscriptions.isEmpty, "membership subscribed twice")
        subscriptions = subscribe(self)
    }

    private func readPeople() {
        let read = readPeopleOperation
        peopleRead.replace("people read") {
            try await read()
        } onSuccess: { [weak self] people in
            guard let self else { return }
            members = people.filter { $0.membership != nil }
            selfProfile =
                people.first { $0.membership?.isSelf == true }?.profile
            error = nil
        } onError: { [weak self] error in
            self?.fail(error)
        }
    }

    private func fail(_ error: any Error) {
        logger.error("membership read failed: \(error.localizedDescription)")
        self.error = DisplayError(error)
    }
}

private final class MembershipSink: MembershipCallback, Sendable {
    private let apply: @MainActor @Sendable (BridgeMembership) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeMembership) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeMembership) {
        Task { @MainActor in apply(value) }
    }
    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }
}

private final class ProfilesSink: ProfilesCallback, Sendable {
    private let apply: @MainActor @Sendable () -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable () -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value _: [BridgePersonProfile]) {
        Task { @MainActor in apply() }
    }
    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }
}

private final class JoinRequestsSink: JoinRequestsCallback, Sendable {
    private let apply: @MainActor @Sendable ([BridgeJoinRequest]) -> Void

    init(apply: @escaping @MainActor @Sendable ([BridgeJoinRequest]) -> Void) {
        self.apply = apply
    }

    func onValue(value: [BridgeJoinRequest]) {
        Task { @MainActor in apply(value) }
    }
}
