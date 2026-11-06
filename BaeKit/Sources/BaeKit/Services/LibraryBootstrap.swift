import Foundation
import Observation

/// One restore or join attempt, with its cancel: a restore's cancellation
/// token, or a join's control, which can also check for approval now.
@MainActor
@Observable
public final class LibraryBootstrap {
    /// Where the running attempt stands, as core says: connecting until it
    /// says otherwise; nil while no attempt runs.
    public var status: BridgeBootstrapStatus? {
        guard let following else { return nil }
        return following.state?.status ?? .connecting
    }
    /// Whether Cancel can still stop the running attempt, as core says: false
    /// while no attempt runs and until core has said so.
    public var cancellable: Bool { following?.state?.cancellable ?? false }
    /// The counted step the running attempt runs, opening or loading, for
    /// the line that draws it; nil while no attempt runs.
    public var steps: LatestSource<BridgeBootstrapStep>? { following?.steps }
    private var following: BootstrapFollowing?
    /// The steps of the open a restore over this device's copy runs when
    /// this device already holds the code's keys; nil for any other attempt.
    public private(set) var openSteps: LatestSource<BridgeOpenProgress>?
    /// The process's host, which runs each attempt and pulls its steps.
    private let host: BridgeHost
    private let deviceName: String
    private var cancelAttempt: (() -> Void)?
    private var joinControl: BridgeJoinControl?
    private var running = false

    public init(host: BridgeHost, deviceName: String) {
        self.host = host
        self.deviceName = deviceName
    }

    public func restore(code: String, credentials: BridgeS3Credentials?)
        async throws -> BridgeBootstrapResult
    {
        try await runRestore { setup, cancellation in
            try await self.host.restoreFromCode(
                code: code,
                credentials: credentials,
                deviceName: self.deviceName,
                setup: setup,
                cancellation: cancellation
            )
        }
    }

    public func restore(
        saved: BridgeSavedRestoreCode,
        credentials: BridgeS3Credentials?
    ) async throws -> BridgeBootstrapResult {
        try await runRestore { setup, cancellation in
            try await self.host.restoreFromSavedCode(
                saved: saved,
                credentials: credentials,
                deviceName: self.deviceName,
                setup: setup,
                cancellation: cancellation
            )
        }
    }

    /// Restore a library this device holds with the person's current restore
    /// code, after another of their devices replaced their keys. The library
    /// is closed and its warning shown before this runs. A failed restore
    /// leaves the library on this device as it was, to open again or to
    /// restore again.
    public func restoreReplacing(
        libraryId: String,
        code: String,
        credentials: BridgeS3Credentials?
    ) async throws -> BridgeBootstrapResult {
        let progress = LibraryOpenProgress(host: host)
        openSteps = progress.steps
        defer { openSteps = nil }
        return try await runRestore { setup, cancellation in
            try await self.host.restoreReplacingDevice(
                libraryId: libraryId,
                code: code,
                credentials: credentials,
                deviceName: self.deviceName,
                setup: setup,
                opening: progress.opening,
                cancellation: cancellation
            )
        }
    }

    /// Finish a restore of this device's copy of a library that stopped
    /// partway, from what core kept on the device: no code, and no sign-in.
    /// Cancelling stops it as it stops any restore; once this device
    /// registered, the restore stays unfinished and the call says so.
    public func finishRestore(libraryId: String) async throws
        -> BridgeBootstrapResult
    {
        try await runRestore { setup, cancellation in
            try await self.host.resumeReplacement(
                libraryId: libraryId,
                by: .restore,
                setup: setup,
                cancellation: cancellation
            )
        }
    }

    /// Join from an invite's code. Cancelling the task drops the call and
    /// keeps the join, which `resume` continues; only `cancel()` cancels it,
    /// and only before its request may have been sent.
    public func join(code: String) async throws -> BridgeBootstrapResult {
        try await runJoin { setup, control in
            try await self.host.joinWithInvite(
                code: code,
                deviceName: self.deviceName,
                setup: setup,
                control: control
            )
        }
    }

    /// Continue a join this device has under way, as `join` does.
    public func resume(_ join: BridgeUnfinishedJoin) async throws
        -> BridgeBootstrapResult
    {
        try await runJoin { setup, control in
            try await self.host.resumeJoin(
                join: join,
                setup: setup,
                control: control
            )
        }
    }

    public func cancel() { cancelAttempt?() }

    /// End the running join's wait so it checks for approval now.
    public func checkNow() { joinControl?.checkNow() }

    private func runRestore(
        _ work: (BridgeLibrarySetup, BootstrapCancellation) async throws ->
            BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        let token = BootstrapCancellation()
        return try await run(cancel: token.cancel) { setup in
            // A published result owns a live store even if cancellation raced
            // it. Always deliver that result so its owner can finish opening
            // or close it.
            try await withTaskCancellationHandler {
                try await work(setup, token)
            } onCancel: {
                token.cancel()
            }
        }
    }

    private func runJoin(
        _ work: (BridgeLibrarySetup, BridgeJoinControl) async throws ->
            BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        let control = BridgeJoinControl()
        joinControl = control
        defer { joinControl = nil }
        return try await run(cancel: control.cancel) { setup in
            try await work(setup, control)
        }
    }

    /// Run one attempt, following where it stands through the setup it is
    /// handed.
    private func run(
        cancel: @escaping () -> Void,
        _ work: (BridgeLibrarySetup) async throws -> BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        precondition(!running, "one bootstrap attempt at a time")
        running = true
        cancelAttempt = cancel
        let following = BootstrapFollowing(
            setup: BridgeLibrarySetup(host: host)
        )
        self.following = following
        defer {
            following.stop()
            self.following = nil
            cancelAttempt = nil
            running = false
        }
        return try await work(following.setup)
    }
}
