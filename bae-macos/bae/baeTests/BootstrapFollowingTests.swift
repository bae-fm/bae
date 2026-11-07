import BaeKit
import Foundation
import Testing

/// Where a call stands, said in place of core: each state the test says
/// answers the next pull.
private final class SaidStates: SetupStatesSubscription, @unchecked Sendable {
    private let lock = NSLock()
    private var said: [BridgeBootstrapState] = []
    private var puller: CheckedContinuation<BridgeBootstrapState, any Error>?

    init() {
        super.init(noHandle: SetupStatesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("said states are made by init()")
    }

    func say(_ state: BridgeBootstrapState) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { said.append(state) }
            return puller
        }
        puller?.resume(returning: state)
    }

    override func next() async throws -> BridgeBootstrapState {
        try await withCheckedThrowingContinuation { continuation in
            let ready: BridgeBootstrapState? = lock.withLock {
                if said.isEmpty {
                    puller = continuation
                    return nil
                }
                return said.removeFirst()
            }
            if let ready { continuation.resume(returning: ready) }
        }
    }

    override func cancel() async throws {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }
}

/// A steps stream that holds nothing, its pull waiting until it is
/// cancelled.
private final class NoSteps: KeyedValueStream, @unchecked Sendable {
    private let lock = NSLock()
    private var puller: CheckedContinuation<[Data], any Error>?

    init() {
        super.init(noHandle: KeyedValueStream.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("an empty stream is made by init()")
    }

    override func latest(key _: Data) -> BridgeFastValue? { nil }

    override func moving(key _: Data) -> Bool { false }

    override func next() async throws -> [Data] {
        try await withCheckedThrowingContinuation { continuation in
            lock.withLock { puller = continuation }
        }
    }

    override func cancel() {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }
}

/// A handle-less setup whose states the test says.
private final class SaidSetup: BridgeLibrarySetup, @unchecked Sendable {
    let states = SaidStates()
    private let steps = NoSteps()

    init() {
        super.init(noHandle: BridgeLibrarySetup.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a said setup is made by init()")
    }

    override func subscribeStates() -> SetupStatesSubscription { states }

    override func feedSetupSteps() -> KeyedValueStream { steps }
}

@MainActor
@Suite("Following a restore, join or reset")
struct BootstrapFollowingTests {
    /// Each state core says reaches the screen as it is said: what the screen
    /// shows changes when the call's state does, with no reading on a clock.
    @Test("each state the call says reaches the screen")
    func eachStateReachesTheScreen() async throws {
        let setup = SaidSetup()
        let following = BootstrapFollowing(setup: setup)
        #expect(following.state == nil)

        let connecting = BridgeBootstrapState(
            status: .connecting,
            cancellable: true
        )
        setup.states.say(connecting)
        try await Wait.until { following.state == connecting }

        let loading = BridgeBootstrapState(status: .loading, cancellable: false)
        setup.states.say(loading)
        try await Wait.until { following.state == loading }

        following.stop()
    }
}
