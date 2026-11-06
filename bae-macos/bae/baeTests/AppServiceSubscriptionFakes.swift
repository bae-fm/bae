import BaeKit
import Foundation

@testable import bae

// The subscriptions the faked `AppHandle` in `AppServiceSubscriptionTests`
// hands out: each answers what its test delivers, or nothing.

/// A cast device list that lists nothing: its one pull answers that it was
/// cancelled, which ends the screen's pulls.
final class NoCastDevices: CastDevicesSubscription, @unchecked Sendable {
    init() {
        super.init(noHandle: CastDevicesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> [BridgeCastDevice] {
        throw BridgeError.Cancelled
    }

    override func cancel() async throws {}
}

/// Values a test hands the app one at a time, each answering the app's next
/// pull, and failed reads in their place; cancelling answers a waiting pull
/// that it was cancelled.
final class PullQueue<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var waiting: [Result<Value, any Error>] = []
    private var puller: CheckedContinuation<Value, any Error>?

    func deliver(_ value: Value) { answer(.success(value)) }

    /// The next pull answers that its read failed with `error`.
    func fail(_ error: any Error) { answer(.failure(error)) }

    private func answer(_ result: Result<Value, any Error>) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { waiting.append(result) }
            return puller
        }
        puller?.resume(with: result)
    }

    func next() async throws -> Value {
        try await withCheckedThrowingContinuation { continuation in
            let result = lock.withLock {
                if waiting.isEmpty {
                    puller = continuation
                    return Result<Value, any Error>?.none
                }
                return waiting.removeFirst()
            }
            if let result { continuation.resume(with: result) }
        }
    }

    func cancel() {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }
}

/// The sync status a test hands the app one value, or one failed read, at a
/// time.
final class FakeSyncStatus: SyncStatusSubscription, @unchecked Sendable {
    let values = PullQueue<BridgeSyncStatusSnapshot>()

    init() {
        super.init(noHandle: SyncStatusSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeSyncStatusSnapshot {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// The sync activity a test hands the app one value at a time.
final class FakeSyncActivity: SyncActivitySubscription,
    @unchecked Sendable
{
    let values = PullQueue<BridgeSyncActivity>()

    init() {
        super.init(noHandle: SyncActivitySubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeSyncActivity {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// This device's open invites, a list a test hands the app at a time.
final class FakeInvites: InvitesSubscription, @unchecked Sendable {
    let values = PullQueue<[BridgeInvite]>()

    init() {
        super.init(noHandle: InvitesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    private let cancelLock = NSLock()
    private var cancelCount = 0
    var cancels: Int { cancelLock.withLock { cancelCount } }

    override func next() async throws -> [BridgeInvite] {
        try await values.next()
    }

    override func cancel() async throws {
        cancelLock.recording { cancelCount += 1 }
        values.cancel()
    }
}

/// Preferences that never change after the ones the service was built with:
/// a pull waits until it is cancelled.
final class SilentConfig: ConfigSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeConfig>()

    init() {
        super.init(noHandle: ConfigSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeConfig {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Playback values a test hands the app one at a time.
final class FakePlaybackValues: PlaybackValuesSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<BridgePlaybackValues>()

    init() {
        super.init(noHandle: PlaybackValuesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ value: BridgePlaybackValues) { values.deliver(value) }

    override func next() async throws -> BridgePlaybackValues {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Timelines a test hands the app one at a time.
final class FakePlaybackTimeline: PlaybackTimelineSubscription,
    @unchecked Sendable
{
    private let timelines = PullQueue<BridgePlaybackTimeline>()

    init() {
        super.init(noHandle: PlaybackTimelineSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ timeline: BridgePlaybackTimeline) {
        timelines.deliver(timeline)
    }

    override func next() async throws -> BridgePlaybackTimeline {
        try await timelines.next()
    }

    override func cancel() async throws { timelines.cancel() }
}

/// Queue values a test hands the app one at a time.
final class FakeQueue: QueueSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeQueueSnapshot>()

    init() {
        super.init(noHandle: QueueSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ value: BridgeQueueSnapshot) { values.deliver(value) }

    override func next() async throws -> BridgeQueueSnapshot {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Whether uploads are paused, which never changes: a pull waits until it
/// is cancelled.
final class SilentUploadsPaused: UploadsPausedSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<Bool>()

    init() {
        super.init(noHandle: UploadsPausedSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> Bool { try await values.next() }

    override func cancel() async throws { values.cancel() }
}

/// The library's name, which never changes: a pull waits until it is
/// cancelled.
final class SilentLibraryName: LibraryNameSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<String>()

    init() {
        super.init(noHandle: LibraryNameSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> String { try await values.next() }

    override func cancel() async throws { values.cancel() }
}

/// The members, who never change: a pull waits until it is cancelled.
final class SilentMembers: MembersSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeMembership>()

    init() {
        super.init(noHandle: MembersSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeMembership {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// The profiles, which never change: a pull waits until it is cancelled.
final class SilentProfiles: ProfilesSubscription, @unchecked Sendable {
    private let values = PullQueue<[BridgePersonProfile]>()

    init() {
        super.init(noHandle: ProfilesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> [BridgePersonProfile] {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// A requested profile that never changes: a pull waits until cancelled.
final class SilentRequestedProfile: RequestedProfileSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<BridgeRequestedProfile?>()

    init() {
        super.init(noHandle: RequestedProfileSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeRequestedProfile? {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Admin changes that never change: a pull waits until cancelled.
final class SilentAdminChanges: AdminChangesSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<[BridgeAdminChange]>()

    init() {
        super.init(noHandle: AdminChangesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> [BridgeAdminChange] {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Queue additions a test hands the app one at a time.
final class FakeQueueAdditions: QueueAdditionsSubscription,
    @unchecked Sendable
{
    private let additions = PullQueue<BridgeQueueAddition>()

    init() {
        super.init(noHandle: QueueAdditionsSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ addition: BridgeQueueAddition) {
        additions.deliver(addition)
    }

    override func next() async throws -> BridgeQueueAddition {
        try await additions.next()
    }

    override func cancel() async throws { additions.cancel() }
}

/// A stream nothing feeds, which holds nothing: a pull waits until the
/// stream is cancelled, then answers that it was.
final class NoopKeyedValueStream: KeyedValueStream,
    @unchecked Sendable
{
    private let keys = PullQueue<[Data]>()

    init() {
        super.init(noHandle: KeyedValueStream.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func latest(key _: Data) -> BridgeFastValue? {
        nil
    }

    override func next() async throws -> [Data] {
        try await keys.next()
    }

    override func cancel() {
        keys.cancel()
    }
}
