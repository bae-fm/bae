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
/// the requested profile and the invites, and to core's joined list of the
/// people: this device's person, the others and their counts. A person's
/// page reads that person by member id, and each place that shows admin
/// changes reads its own changes by the place.
@MainActor
@Observable
public final class MembershipStore {
    /// The people in the library with their profiles: this device's person,
    /// the others and how many people and devices there are, or nil before
    /// the first read.
    public private(set) var people: BridgePeople?
    /// The person whose page is open, by their member id: no person once
    /// they have left the library.
    public let person = KeyedRead<String, BridgePerson?>()
    public private(set) var selfIsAdmin = false
    /// This device's person's profile, nil until they have one.
    public var selfProfile: BridgePersonProfile? { people?.own?.profile }
    /// The profile this device's person asked for, until it is written.
    public private(set) var requestedProfile: BridgeRequestedProfile?
    /// The rows of People's invites in the order they were made: this
    /// device's open invites, each with its request once it arrives, and the
    /// invites a sharing refusal ended, until the person dismisses them.
    public private(set) var inviteEntries: [BridgeInviteEntry] = []
    /// The first open invite whose join request waits for a decision,
    /// other than those of the invites whose request the page closed, while
    /// the page reads it: no invite while none waits.
    public let waitingRequest = KeyedRead<Set<String>, BridgeInvite?>()
    /// The open invite whose sheet shows, by its id: no invite once it has
    /// ended.
    public let invite = KeyedRead<String, BridgeInvite?>()
    /// The admin changes this device asked for that show with the library's
    /// name, each until the person dismisses its outcome.
    private let libraryAdminChanges =
        KeyedRead<BridgeAdminChangePlace, BridgeAdminChanges>()
    /// The admin changes that show on the page of People and devices that is
    /// open: People, or one person's page.
    private let pageAdminChanges =
        KeyedRead<BridgeAdminChangePlace, BridgeAdminChanges>()
    /// The last failed read; cleared by the next successful one.
    public private(set) var error: DisplayError?
    /// Where the pairing `followAddDevice` follows is, nil until it reports.
    public private(set) var addDeviceStatus: BridgeAddDeviceStatus?

    /// Starts the membership subscription, returning what keeps it running.
    private let subscribeMembers: @MainActor (MembershipStore) -> AnyObject
    /// Starts the people, requested profile, admin change and invite
    /// subscriptions a screen shows, returning what keeps each running.
    private let subscribeShown: @MainActor (MembershipStore) -> [AnyObject]
    /// Starts the subscription to the person with the member id.
    private let subscribePerson: Subscribe<String, BridgePerson?>
    /// Starts the subscription to the first waiting join request other than
    /// those of the invites with the ids.
    private let subscribeWaitingRequest: Subscribe<Set<String>, BridgeInvite?>
    /// Starts the subscription to the admin changes that show in the place.
    private let subscribeAdminChanges:
        Subscribe<BridgeAdminChangePlace, BridgeAdminChanges>
    /// Starts the subscription to the open invite with the id.
    private let subscribeInvite: Subscribe<String, BridgeInvite?>
    /// Starts the pull of a pairing's status, handing each value to the
    /// closure; the subscription runs until the closure it returns stops it.
    private let followAddDeviceStatus:
        @MainActor (
            String,
            @escaping @MainActor (BridgeAddDeviceStatus) -> Void,
            @escaping @MainActor (any Error) -> Void
        ) throws -> @MainActor () -> Void
    /// Asks this device to sync every few seconds, returning what ends the
    /// request.
    private let holdFrequentSync: @MainActor () -> () -> Void
    @ObservationIgnored
    private var membersSubscription: AnyObject?
    /// What a screen showing the people reads, while one does.
    @ObservationIgnored
    private var shownSubscriptions: [AnyObject]?

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
            subscribeShown: { store in Self.followShown(handle, store: store) },
            subscribePerson: { store, memberId, apply in
                Self.follow(
                    try handle.subscribePerson(member: memberId),
                    store: store,
                    apply: apply
                )
            },
            subscribeWaitingRequest: { store, closed, apply in
                Self.follow(
                    try handle.subscribeWaitingJoinRequest(
                        closed: Array(closed)
                    ),
                    store: store,
                    apply: apply
                )
            },
            subscribeAdminChanges: { store, place, apply in
                Self.follow(
                    try handle.subscribeAdminChanges(place: place),
                    store: store,
                    apply: apply
                )
            },
            subscribeInvite: { store, inviteId, apply in
                Self.follow(
                    try handle.subscribeInvite(invite: inviteId),
                    store: store,
                    apply: apply
                )
            },
            followAddDeviceStatus: { pairing, apply, fail in
                let subscription = ScreenSubscription(
                    try handle.followAddDevice(pairing: pairing),
                    onValue: apply,
                    onError: fail
                )
                return { subscription.stop() }
            },
            holdFrequentSync: {
                let hold = handle.holdInviteScreenSync()
                return { hold.release() }
            }
        )
    }

    private init(
        subscribeMembers: @escaping @MainActor (MembershipStore) -> AnyObject,
        subscribeShown: @escaping @MainActor (MembershipStore) -> [AnyObject],
        subscribePerson: @escaping Subscribe<String, BridgePerson?>,
        subscribeWaitingRequest:
            @escaping Subscribe<Set<String>, BridgeInvite?>,
        subscribeAdminChanges:
            @escaping Subscribe<BridgeAdminChangePlace, BridgeAdminChanges>,
        subscribeInvite: @escaping Subscribe<String, BridgeInvite?>,
        followAddDeviceStatus:
            @escaping @MainActor (
                String,
                @escaping @MainActor (BridgeAddDeviceStatus) -> Void,
                @escaping @MainActor (any Error) -> Void
            ) throws -> @MainActor () -> Void,
        holdFrequentSync: @escaping @MainActor () -> () -> Void
    ) {
        self.holdFrequentSync = holdFrequentSync
        self.subscribeMembers = subscribeMembers
        self.subscribeShown = subscribeShown
        self.subscribePerson = subscribePerson
        self.subscribeWaitingRequest = subscribeWaitingRequest
        self.subscribeInvite = subscribeInvite
        self.subscribeAdminChanges = subscribeAdminChanges
        self.followAddDeviceStatus = followAddDeviceStatus
    }

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
        show(person, memberId, subscribePerson)
    }

    /// End what `showPerson` reads, once no person's page shows.
    public func hidePerson() {
        person.hide()
    }

    /// Read the first waiting join request other than those of the invites
    /// in `closed`, replacing the read of an earlier `closed`.
    public func showWaitingRequest(closed: Set<String>) {
        show(waitingRequest, closed, subscribeWaitingRequest)
    }

    /// End what `showWaitingRequest` reads, once the page closes.
    public func hideWaitingRequest() {
        waitingRequest.hide()
    }

    /// End what `show` reads, once no screen shows it. What it read goes
    /// with it, so the next screen starts from its own read.
    public func hide() {
        guard shownSubscriptions != nil else { return }
        hidePerson()
        hideWaitingRequest()
        hideInvite()
        shownSubscriptions = nil
        people = nil
        requestedProfile = nil
        inviteEntries = []
        libraryAdminChanges.hide()
        pageAdminChanges.hide()
        error = nil
    }

    private func membersChanged(_ membership: BridgeMembership) {
        selfIsAdmin = membership.selfIsAdmin
    }

    /// Follow the pairing `pairing` until the calling task is cancelled, then
    /// stop following it and end it with `end`: Add Device's sheet runs it
    /// while it shows the pairing's code. The store stops reading the
    /// pairing's status before it calls `end`, so the sheet never shows the
    /// forgotten status that coven reports after the pairing ends, and keeps
    /// what it showed last.
    public func followAddDevice(
        _ pairing: String,
        endingWith end: @escaping @Sendable (String) async throws -> Void
    ) async {
        addDeviceStatus = nil
        var stop: (@MainActor () -> Void)?
        do {
            stop = try followAddDeviceStatus(
                pairing,
                { [weak self] status in self?.addDeviceStatus = status },
                { [weak self] error in self?.fail(error) }
            )
        }
        catch {
            fail(error)
        }
        await Task.untilCancelled()
        stop?()
        Task { await endAddDevice(pairing, with: end) }
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

    /// Start `read` of `key` through `subscribe`, showing a read that could
    /// not start as the store's error.
    private func show<Key, Value>(
        _ read: KeyedRead<Key, Value>,
        _ key: Key,
        _ subscribe: Subscribe<Key, Value>
    ) {
        do {
            try read.show(key) { apply in try subscribe(self, key, apply) }
        }
        catch {
            fail(error)
        }
    }

    private func fail(_ error: any Error) {
        logger.error("membership read failed: \(String(reflecting: error))")
        self.error = DisplayError(error)
    }
}

extension MembershipStore {
    /// The subscriptions to what a screen showing the people reads, which
    /// hand each value to `store`.
    private static func followShown(
        _ handle: AppHandle,
        store: MembershipStore
    ) -> [AnyObject] {
        [
            ScreenSubscription(
                handle.subscribePeople(),
                onValue: { [weak store] people in store?.people = people },
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
                onValue: { [weak store] entries in
                    store?.inviteEntries = entries
                },
                onError: { [weak store] error in store?.fail(error) }
            ),
        ]
    }

    /// The subscription that hands each value `subscription` pulls to
    /// `apply` and each failed read to `store`.
    private static func follow<Subscription: PulledSubscription>(
        _ subscription: Subscription,
        store: MembershipStore,
        apply: @escaping @MainActor @Sendable (Subscription.Value) -> Void
    ) -> AnyObject {
        ScreenSubscription(
            subscription,
            onValue: apply,
            onError: { [weak store] error in store?.fail(error) }
        )
    }

    /// The read of the admin changes that show in `place`: the library's own
    /// read, or the read of the page of People and devices that is open.
    private func adminChangesRead(
        _ place: BridgeAdminChangePlace
    ) -> KeyedRead<BridgeAdminChangePlace, BridgeAdminChanges> {
        place == .library ? libraryAdminChanges : pageAdminChanges
    }

    /// The admin changes that show in `place` as its read returned them;
    /// nil before the read returns.
    public func adminChanges(
        at place: BridgeAdminChangePlace
    ) -> BridgeAdminChanges? {
        guard let shown = adminChangesRead(place).shown, shown.key == place
        else { return nil }
        return shown.value
    }

    /// Read the admin changes that show in `place` while its page shows them.
    public func showAdminChanges(at place: BridgeAdminChangePlace) {
        show(adminChangesRead(place), place, subscribeAdminChanges)
    }

    /// End the read of the admin changes that show in `place`, once its page
    /// closes.
    public func hideAdminChanges(at place: BridgeAdminChangePlace) {
        adminChangesRead(place).hide()
    }

    /// Read the open invite with the id `inviteId` while its sheet shows.
    public func showInvite(_ inviteId: String) {
        show(invite, inviteId, subscribeInvite)
    }

    /// End what `showInvite` reads, once the invite's sheet closes.
    public func hideInvite() {
        invite.hide()
    }

    #if DEBUG
        /// A store showing `invites` and `refusedInvites` and `people` from the
        /// start, with no subscriptions; a page reading a place's admin
        /// changes finds that place's in `adminChanges`, and following a
        /// pairing finds `addDeviceStatus`.
        public static func stub(
            people: BridgePeople,
            selfIsAdmin: Bool,
            invites: [BridgeInvite] = [],
            refusedInvites: [BridgeRefusedInvite] = [],
            requestedProfile: BridgeRequestedProfile? = nil,
            adminChanges: [BridgeAdminChangePlace: [BridgeAdminChange]] = [:],
            addDeviceStatus: BridgeAddDeviceStatus? = nil
        ) -> MembershipStore {
            let store = MembershipStore(
                subscribeMembers: { _ in NSObject() },
                subscribeShown: { _ in [] },
                subscribePerson: { _, memberId, apply in
                    apply(
                        ([people.own].compactMap { $0 } + people.others)
                            .first { $0.membership.id == memberId }
                    )
                    return NSObject()
                },
                subscribeWaitingRequest: { _, _, _ in NSObject() },
                subscribeAdminChanges: { _, place, apply in
                    // Core answers whether there is a change; the stub
                    // states it for its fixture.
                    let changes = adminChanges[place] ?? []
                    apply(
                        BridgeAdminChanges(
                            changes: changes,
                            any: !changes.isEmpty
                        )
                    )
                    return NSObject()
                },
                subscribeInvite: { _, inviteId, apply in
                    apply(invites.first { $0.id == inviteId })
                    return NSObject()
                },
                followAddDeviceStatus: { _, apply, _ in
                    if let addDeviceStatus { apply(addDeviceStatus) }
                    return {}
                },
                holdFrequentSync: { {} }
            )
            store.selfIsAdmin = selfIsAdmin
            store.people = people
            store.inviteEntries =
                invites.map { .open(invite: $0) }
                + refusedInvites.map { .refused(refused: $0) }
            store.requestedProfile = requestedProfile
            return store
        }
    #endif
}

/// Starts the subscription of a read by key: it hands each value it pulls
/// to the closure and each failed read to the store, and returns what keeps
/// it running.
typealias Subscribe<Key, Value> =
    @MainActor (
        MembershipStore, Key, @escaping @MainActor @Sendable (Value) -> Void
    ) throws -> AnyObject

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
    /// coven no longer holds the pairing: the sheet says its code has ended,
    /// as for an expired one.
    case forgotten
}

/// End the pairing `pairing` with `end`. Ending a pairing is a write to this
/// device's own database that needs no storage, so a failure is a bug: the
/// sheet has closed, and the pairing's code stays live in coven until it
/// expires.
private func endAddDevice(
    _ pairing: String,
    with end: @Sendable (String) async throws -> Void
) async {
    do { try await end(pairing) }
    catch {
        let detail = "ending the pairing failed: \(String(reflecting: error))"
        logger.error("\(detail)")
        CoreBugReporter()
            .report(operation: "MembershipStore.endAddDevice", detail: detail)
    }
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
        case .forgotten: return .forgotten
        }
    }
}
