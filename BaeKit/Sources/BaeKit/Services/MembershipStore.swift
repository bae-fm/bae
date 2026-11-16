import Foundation
import Observation
import os.log

private let logger = Logger.bae("MembershipStore")

/// The store's members joined with their profiles, the profile this device's
/// person asked for, the invites this device has open and the admin changes
/// it asked for. The store reads whether this device's person is an admin
/// for as long as the library is open, from `start()`: the menu bar always
/// shows Rename Library, which it enables. The store reads the rest while a
/// screen shows it, from `show()` to `hide()`: it subscribes to the people,
/// the requested profile, invites and admin changes, and the people are one
/// subscription of core's joined list: this device's person, the others and
/// their counts. A person's page follows its own subscription, keyed by
/// their member id.
@MainActor
@Observable
public final class MembershipStore {
    /// The people in the library with their profiles: this device's person,
    /// the others and how many people and devices there are, or nil before
    /// the first read.
    public private(set) var people: BridgePeople?
    /// The person whose page is open, once their read arrives.
    public private(set) var shownPerson: ShownPerson?
    public private(set) var selfIsAdmin = false
    /// This device's person's profile, nil until they have one.
    public var selfProfile: BridgePersonProfile? { people?.own?.profile }
    /// The profile this device's person asked for, until it is written.
    public private(set) var requestedProfile: BridgeRequestedProfile?
    /// The rows of People's invites in the order they were made: this
    /// device's open invites, each with its request once it arrives, and the
    /// invites a sharing refusal ended, until the person dismisses them.
    public private(set) var inviteEntries: [BridgeInviteEntry] = []
    /// This device's open invites.
    public var invites: [BridgeInvite] {
        inviteEntries.compactMap {
            if case .open(let invite) = $0 {
                invite
            }
            else {
                nil
            }
        }
    }
    /// The admin changes this device asked for, each until the person
    /// dismisses its outcome.
    public private(set) var adminChanges: [BridgeAdminChange] = []
    /// The last failed read; cleared by the next successful one.
    public private(set) var error: DisplayError?
    /// Where the pairing `followAddDevice` follows is, nil until it reports.
    public private(set) var addDeviceStatus: BridgeAddDeviceStatus?

    /// Starts the membership subscription, returning what keeps it running.
    private let subscribeMembers: @MainActor (MembershipStore) -> AnyObject
    /// Starts the people, requested profile, admin change and invite
    /// subscriptions a screen shows, returning what keeps each running.
    private let subscribeShown: @MainActor (MembershipStore) -> [AnyObject]
    /// Starts the subscription to the person with the member id, handing
    /// each value to the store.
    private let subscribePerson:
        @MainActor (MembershipStore, String) throws -> AnyObject
    /// Starts the pull of a pairing's status, handing each value to the
    /// closure; the subscription runs while what it returns is held.
    private let followAddDeviceStatus:
        @MainActor (
            String,
            @escaping @MainActor (BridgeAddDeviceStatus) -> Void,
            @escaping @MainActor (any Error) -> Void
        ) throws -> AnyObject
    /// Asks this device to sync every few seconds, returning what ends the
    /// request.
    private let holdFrequentSync: @MainActor () -> () -> Void
    @ObservationIgnored
    private var membersSubscription: AnyObject?
    /// What a screen showing the people reads, while one does.
    @ObservationIgnored
    private var shownSubscriptions: [AnyObject]?
    /// What a person's page reads, with the member id it reads.
    @ObservationIgnored
    private var personSubscription: (memberId: String, subscription: AnyObject)?

    public convenience init(handle: AppHandle) {
        self.init(
            subscribeMembers: { store in
                ScreenSubscription(
                    handle.subscribeMembers(),
                    onValue: { [weak store] membership in
                        store?.membersChanged(membership)
                    },
                    onError: { [weak store] error in store?.fail(error) }
                )
            },
            subscribeShown: { store in
                [
                    ScreenSubscription(
                        handle.subscribePeople(),
                        onValue: { [weak store] people in store?.people = people
                        },
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
                        handle.subscribeAdminChanges(),
                        onValue: { [weak store] changes in
                            store?.adminChanges = changes
                        },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                    ScreenSubscription(
                        handle.subscribeInvites(),
                        onValue: { [weak store] entries in
                            store?.inviteEntries = entries
                        },
                        onError: { [weak store] error in store?.fail(error) }
                    ),
                ]
            },
            subscribePerson: { store, memberId in
                try Self.followPerson(handle, store: store, memberId: memberId)
            },
            followAddDeviceStatus: { pairing, apply, fail in
                ScreenSubscription(
                    try handle.followAddDevice(pairing: pairing),
                    onValue: apply,
                    onError: fail
                )
            },
            holdFrequentSync: {
                let hold = handle.holdInviteScreenSync()
                return { hold.release() }
            }
        )
    }

    /// The subscription to the person with `memberId`, which hands each value
    /// to `store`.
    private static func followPerson(
        _ handle: AppHandle,
        store: MembershipStore,
        memberId: String
    ) throws -> AnyObject {
        ScreenSubscription(
            try handle.subscribePerson(member: memberId),
            onValue: { [weak store] person in
                store?.shownPerson = ShownPerson(
                    memberId: memberId,
                    person: person
                )
            },
            onError: { [weak store] error in store?.fail(error) }
        )
    }

    private init(
        subscribeMembers: @escaping @MainActor (MembershipStore) -> AnyObject,
        subscribeShown: @escaping @MainActor (MembershipStore) -> [AnyObject],
        subscribePerson:
            @escaping @MainActor (MembershipStore, String) throws -> AnyObject,
        followAddDeviceStatus:
            @escaping @MainActor (
                String,
                @escaping @MainActor (BridgeAddDeviceStatus) -> Void,
                @escaping @MainActor (any Error) -> Void
            ) throws -> AnyObject,
        holdFrequentSync: @escaping @MainActor () -> () -> Void
    ) {
        self.holdFrequentSync = holdFrequentSync
        self.subscribeMembers = subscribeMembers
        self.subscribeShown = subscribeShown
        self.subscribePerson = subscribePerson
        self.followAddDeviceStatus = followAddDeviceStatus
    }

    #if DEBUG
        /// A store showing `invites` and `refusedInvites` and `people` from the
        /// start, with no subscriptions; following a pairing finds
        /// `addDeviceStatus`.
        public static func stub(
            people: BridgePeople,
            selfIsAdmin: Bool,
            invites: [BridgeInvite] = [],
            refusedInvites: [BridgeRefusedInvite] = [],
            requestedProfile: BridgeRequestedProfile? = nil,
            adminChanges: [BridgeAdminChange] = [],
            addDeviceStatus: BridgeAddDeviceStatus? = nil
        ) -> MembershipStore {
            let store = MembershipStore(
                subscribeMembers: { _ in NSObject() },
                subscribeShown: { _ in [] },
                subscribePerson: { store, memberId in
                    store.shownPerson = ShownPerson(
                        memberId: memberId,
                        person: ([people.own].compactMap { $0 } + people.others)
                            .first { $0.membership.id == memberId }
                    )
                    return NSObject()
                },
                followAddDeviceStatus: { _, apply, _ in
                    if let addDeviceStatus { apply(addDeviceStatus) }
                    return NSObject()
                },
                holdFrequentSync: { {} }
            )
            store.selfIsAdmin = selfIsAdmin
            store.people = people
            store.inviteEntries =
                invites.map { .open(invite: $0) }
                + refusedInvites.map { .refused(refused: $0) }
            store.requestedProfile = requestedProfile
            store.adminChanges = adminChanges
            return store
        }
    #endif

    /// Read whether this device's person is an admin, for as long as the
    /// library is open.
    public func start() {
        precondition(membersSubscription == nil, "membership subscribed twice")
        membersSubscription = subscribeMembers(self)
    }

    /// Read the people, invites and admin changes while a screen shows them.
    public func show() {
        guard shownSubscriptions == nil else { return }
        shownSubscriptions = subscribeShown(self)
    }

    /// Read the person with the member id `memberId` while their page shows.
    public func showPerson(_ memberId: String) {
        guard personSubscription?.memberId != memberId else { return }
        hidePerson()
        do {
            personSubscription = (
                memberId, try subscribePerson(self, memberId)
            )
        }
        catch {
            fail(error)
        }
    }

    /// End what `showPerson` reads, once no person's page shows.
    public func hidePerson() {
        personSubscription = nil
        shownPerson = nil
    }

    /// End what `show` reads, once no screen shows it. What it read goes
    /// with it, so the next screen starts from its own read.
    public func hide() {
        guard shownSubscriptions != nil else { return }
        hidePerson()
        shownSubscriptions = nil
        people = nil
        requestedProfile = nil
        inviteEntries = []
        adminChanges = []
        error = nil
    }

    private func membersChanged(_ membership: BridgeMembership) {
        selfIsAdmin = membership.selfIsAdmin
    }

    /// Follow the pairing `pairing` until the calling task is cancelled: Add
    /// Device's sheet runs it while it shows the pairing's code.
    public func followAddDevice(_ pairing: String) async {
        addDeviceStatus = nil
        let pulls: AnyObject
        do {
            pulls = try followAddDeviceStatus(
                pairing,
                { [weak self] status in self?.addDeviceStatus = status },
                { [weak self] error in self?.fail(error) }
            )
        }
        catch {
            fail(error)
            return
        }
        // Held until the task is cancelled, which ends the stream's wait.
        let (cancelled, holding) = AsyncStream.makeStream(of: Never.self)
        for await _ in cancelled {}
        holding.finish()
        withExtendedLifetime(pulls) {}
    }

    /// Have this device sync every few seconds until the calling task is
    /// cancelled: an invite's sheet runs it while it shows, so it hears of the
    /// invite's person soon after their request is sent.
    public func holdInviteScreenSync() async {
        let release = holdFrequentSync()
        defer { release() }
        // Held until the task is cancelled, which ends the stream's wait.
        let (cancelled, holding) = AsyncStream.makeStream(of: Never.self)
        for await _ in cancelled {}
        holding.finish()
    }

    private func fail(_ error: any Error) {
        logger.error("membership read failed: \(String(reflecting: error))")
        self.error = DisplayError(error)
    }
}

/// The person a person's page shows, with the member id it asked for: no
/// person once they have left the library.
public struct ShownPerson {
    public let memberId: String
    public let person: BridgePerson?
}

extension BridgeInviteEntry {
    /// What identifies the row among People's invites.
    public var rowId: String {
        switch self {
        case .open(let invite): invite.id
        case .refused(let refused): "refused-\(refused.operation)"
        }
    }

    /// The invite a sharing refusal ended, if this row is one.
    public var refusedInvite: BridgeRefusedInvite? {
        if case .refused(let refused) = self {
            refused
        }
        else {
            nil
        }
    }
}

/// What Add Device's sheet shows of a pairing's status, as plain values.
public enum AddDeviceFace: Equatable, Sendable {
    /// The code to scan or type: no request has come.
    case code
    /// A new device asks to be added, showing `confirmation`.
    case request(deviceName: String, confirmation: String)
    /// The approved device is on its way in.
    case adding(name: String)
    /// The device is in.
    case added(name: String)
    /// The person declined the request: the sheet closes.
    case declined
    /// The code's time passed with no device added: the sheet says so.
    case expired
    /// coven refused the request, so no device was added: the sheet says so.
    case refused
}

extension BridgeAddDeviceStatus {
    /// What the sheet shows for this status.
    public var face: AddDeviceFace {
        switch self {
        case .waiting(let request?):
            let info = request.info()
            return .request(
                deviceName: info.deviceName,
                confirmation: info.confirmation
            )
        case .waiting(nil): return .code
        case .approved(let name): return .adding(name: name)
        case .ended(.added(let name)): return .added(name: name)
        case .ended(.declined): return .declined
        case .ended(.expired): return .expired
        case .ended(.refused): return .refused
        }
    }
}
