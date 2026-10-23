import Foundation
import Observation
import os.log

private let logger = Logger.bae("MembershipStore")

/// The store's members joined with their profiles, and the invites this
/// device has open. Membership, profiles and invites are subscribed; each
/// change of membership or profiles rereads core's joined list of people.
@MainActor
@Observable
public final class MembershipStore {
    /// The current members with their profiles, or nil before the first read.
    public private(set) var members: [BridgePerson]?
    public private(set) var selfIsAdmin = false
    /// This device's person's profile, nil until they have one.
    public private(set) var selfProfile: BridgePersonProfile?
    /// This device's open invites, each with a sent invite's request once
    /// it arrives.
    public private(set) var invites: [BridgeInvite] = []
    /// The last failed read; cleared by the next successful one.
    public private(set) var error: DisplayError?

    /// Starts the membership, profile and invite subscriptions.
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
                    handle.subscribeInvites(
                        callback: InvitesSink { [weak store] invites in
                            store?.invites = invites
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
        /// A store showing `people` and `invites` from the start, with no
        /// subscriptions.
        public static func stub(
            people: [BridgePerson],
            selfIsAdmin: Bool,
            invites: [BridgeInvite] = []
        ) -> MembershipStore {
            let store = MembershipStore(
                subscribe: { _ in [] },
                readPeople: { people }
            )
            store.selfIsAdmin = selfIsAdmin
            store.invites = invites
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

private final class InvitesSink: InvitesCallback, Sendable {
    private let apply: @MainActor @Sendable ([BridgeInvite]) -> Void

    init(apply: @escaping @MainActor @Sendable ([BridgeInvite]) -> Void) {
        self.apply = apply
    }

    func onValue(value: [BridgeInvite]) {
        Task { @MainActor in apply(value) }
    }
}
