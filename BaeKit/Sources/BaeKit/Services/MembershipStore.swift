import Foundation
import Observation
import os.log

private let logger = Logger.bae("MembershipStore")

/// The store's members joined with their profiles, the profile this device's
/// person asked for, and the invites this device has open. Membership,
/// profiles, the requested profile and invites are subscribed; each
/// change of membership or profiles rereads core's joined list of people and
/// this device's person.
@MainActor
@Observable
public final class MembershipStore {
    /// The current members with their profiles, or nil before the first read.
    public private(set) var members: [BridgePerson]?
    public private(set) var selfIsAdmin = false
    /// This device's person's profile, nil until they have one.
    public private(set) var selfProfile: BridgePersonProfile?
    /// The profile this device's person asked for, until it is written.
    public private(set) var requestedProfile: BridgeRequestedProfile?
    /// This device's open invites, each with a sent invite's request once
    /// it arrives.
    public private(set) var invites: [BridgeInvite] = []
    /// The last failed read; cleared by the next successful one.
    public private(set) var error: DisplayError?
    /// This person's device that `watchForAddedDevice` found, nil until one
    /// joins.
    public private(set) var addedDevice: BridgeActiveDevice?

    /// Starts the membership, profile and invite subscriptions, returning
    /// what keeps each running.
    private let subscribe: @MainActor (MembershipStore) -> [AnyObject]
    /// Starts the pull of this person's device that a watch finds, handing
    /// each value to the closure; the subscription runs while what it
    /// returns is held.
    private let subscribeAddedDevice:
        @MainActor (
            BridgeAddedDeviceWatch,
            @escaping @MainActor (BridgeActiveDevice?) -> Void,
            @escaping @MainActor (any Error) -> Void
        ) -> AnyObject
    private let readPeopleOperation: @Sendable () async throws -> PeopleRead
    @ObservationIgnored
    private var subscriptions: [AnyObject] = []
    @ObservationIgnored
    private let peopleRead = CancellableTaskSlot()

    public convenience init(handle: AppHandle) {
        self.init(
            subscribe: { store in
                [
                    ScreenSubscription(
                        handle.subscribeMembers(),
                        onValue: { [weak store] membership in
                            store?.selfIsAdmin = membership.selfIsAdmin
                            store?.readPeople()
                        },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                    ScreenSubscription(
                        handle.subscribeProfiles(),
                        onValue: { [weak store] _ in store?.readPeople() },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                    ScreenSubscription(
                        handle.subscribeRequestedProfile(),
                        onValue: { [weak store] requested in
                            store?.requestedProfile = requested
                        },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                    ScreenSubscription(
                        handle.subscribeInvites(),
                        onValue: { [weak store] invites in
                            store?.invites = invites
                        },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                ]
            },
            readPeople: {
                async let people = handle.people()
                async let own = handle.ownPerson()
                return PeopleRead(people: try await people, own: try await own)
            },
            subscribeAddedDevice: { watch, apply, fail in
                ScreenSubscription(
                    handle.subscribeAddedDevice(watch: watch),
                    onValue: apply,
                    onError: fail
                )
            }
        )
    }

    private init(
        subscribe: @escaping @MainActor (MembershipStore) -> [AnyObject],
        readPeople: @escaping @Sendable () async throws -> PeopleRead,
        subscribeAddedDevice:
            @escaping @MainActor (
                BridgeAddedDeviceWatch,
                @escaping @MainActor (BridgeActiveDevice?) -> Void,
                @escaping @MainActor (any Error) -> Void
            ) -> AnyObject
    ) {
        self.subscribe = subscribe
        readPeopleOperation = readPeople
        self.subscribeAddedDevice = subscribeAddedDevice
    }

    #if DEBUG
        /// A store showing `people` and `invites` from the start, with no
        /// subscriptions; watching for an added device finds `addedDevice`.
        public static func stub(
            people: [BridgePerson],
            selfIsAdmin: Bool,
            invites: [BridgeInvite] = [],
            requestedProfile: BridgeRequestedProfile? = nil,
            addedDevice: BridgeActiveDevice? = nil
        ) -> MembershipStore {
            let store = MembershipStore(
                subscribe: { _ in [] },
                readPeople: {
                    // The fixture's own person is the one it marks as this
                    // device's, as core would read it.
                    PeopleRead(
                        people: people,
                        own: people.first { $0.membership.isSelf }
                    )
                },
                subscribeAddedDevice: { _, apply, _ in
                    apply(addedDevice)
                    return NSObject()
                }
            )
            store.selfIsAdmin = selfIsAdmin
            store.invites = invites
            store.requestedProfile = requestedProfile
            store.readPeople()
            return store
        }
    #endif

    public func start() {
        precondition(subscriptions.isEmpty, "membership subscribed twice")
        subscriptions = subscribe(self)
    }

    /// Watch for this person's device that `watch` finds, until the calling
    /// task is cancelled: Add Device's sheet runs it while it shows the code
    /// `watch` came with.
    public func watchForAddedDevice(_ watch: BridgeAddedDeviceWatch) async {
        addedDevice = nil
        let pulls = subscribeAddedDevice(
            watch,
            { [weak self] device in self?.addedDevice = device },
            { [weak self] error in self?.fail(error) }
        )
        // Held until the task is cancelled, which ends the stream's wait.
        let (cancelled, holding) = AsyncStream.makeStream(of: Never.self)
        for await _ in cancelled {}
        holding.finish()
        withExtendedLifetime(pulls) {}
    }

    private func readPeople() {
        let read = readPeopleOperation
        peopleRead.replace("people read") {
            try await read()
        } onSuccess: { [weak self] read in
            guard let self else { return }
            members = read.people
            selfProfile = read.own?.profile
            error = nil
        } onError: { [weak self] error in
            self?.fail(error)
        }
    }

    private func fail(_ error: any Error) {
        logger.error("membership read failed: \(String(reflecting: error))")
        self.error = DisplayError(error)
    }
}

/// One read of the library's people: everyone in it, and this device's
/// person among them once their registration has applied.
private struct PeopleRead: Sendable {
    let people: [BridgePerson]
    let own: BridgePerson?
}
