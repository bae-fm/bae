import Foundation
import Observation

/// One restore or join attempt, with its cancel: a restore's cancellation
/// token, or a join's control, which can also check for approval now.
@MainActor
@Observable
public final class LibraryBootstrap {
    public private(set) var status: BridgeBootstrapStatus?
    private let host: BridgeHost
    private let deviceName: String
    private var cancelAttempt: (() -> Void)?
    private var joinControl: BridgeJoinControl?
    private var attempt: UUID?

    public init(host: BridgeHost, deviceName: String) {
        self.host = host
        self.deviceName = deviceName
    }

    public func restore(code: String, credentials: BridgeS3Credentials?)
        async throws -> BridgeBootstrapResult
    {
        try await runRestore { callback, cancellation in
            try await self.host.restoreFromCode(
                code: code,
                credentials: credentials,
                deviceName: self.deviceName,
                callback: callback,
                cancellation: cancellation
            )
        }
    }

    public func restore(
        saved: BridgeSavedRestoreCode,
        credentials: BridgeS3Credentials?
    ) async throws -> BridgeBootstrapResult {
        try await runRestore { callback, cancellation in
            try await self.host.restoreFromSavedCode(
                saved: saved,
                credentials: credentials,
                deviceName: self.deviceName,
                callback: callback,
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
        try await runRestore { callback, cancellation in
            try await self.host.restoreReplacingDevice(
                libraryId: libraryId,
                code: code,
                credentials: credentials,
                deviceName: self.deviceName,
                callback: callback,
                cancellation: cancellation
            )
        }
    }

    /// Join from an invite's code. Cancelling the task drops the call and
    /// keeps the join, which `resume` continues; only `cancel()` cancels it,
    /// and only before its request may have been sent.
    public func join(code: String) async throws -> BridgeBootstrapResult {
        try await runJoin { callback, control in
            try await self.host.joinWithInvite(
                code: code,
                deviceName: self.deviceName,
                callback: callback,
                control: control
            )
        }
    }

    /// Continue a join this device has under way, as `join` does.
    public func resume(_ join: BridgeUnfinishedJoin) async throws
        -> BridgeBootstrapResult
    {
        try await runJoin { callback, control in
            try await self.host.resumeJoin(
                join: join,
                callback: callback,
                control: control
            )
        }
    }

    public func cancel() { cancelAttempt?() }

    /// End the running join's wait so it checks for approval now.
    public func checkNow() { joinControl?.checkNow() }

    private func runRestore(
        _ work: (BootstrapStatusSink, BootstrapCancellation) async throws ->
            BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        let token = BootstrapCancellation()
        return try await run(cancel: token.cancel) { sink in
            // A published result owns a live store even if cancellation raced
            // it. Always deliver that result so its owner can finish opening
            // or close it.
            try await withTaskCancellationHandler {
                try await work(sink, token)
            } onCancel: {
                token.cancel()
            }
        }
    }

    private func runJoin(
        _ work: (BootstrapStatusSink, BridgeJoinControl) async throws ->
            BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        let control = BridgeJoinControl()
        joinControl = control
        defer { joinControl = nil }
        return try await run(cancel: control.cancel) { sink in
            try await work(sink, control)
        }
    }

    private func run(
        cancel: @escaping () -> Void,
        _ work: (BootstrapStatusSink) async throws -> BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        precondition(attempt == nil, "one bootstrap attempt at a time")
        let identity = UUID()
        cancelAttempt = cancel
        attempt = identity
        status = .connecting
        let sink = BootstrapStatusSink { [weak self] status in
            guard self?.attempt == identity else { return }
            self?.status = status
        }
        defer {
            attempt = nil
            cancelAttempt = nil
            status = nil
        }
        return try await work(sink)
    }
}

private final class BootstrapStatusSink: BootstrapCallback, Sendable {
    private let apply: @MainActor @Sendable (BridgeBootstrapStatus) -> Void
    init(apply: @escaping @MainActor @Sendable (BridgeBootstrapStatus) -> Void)
    { self.apply = apply }
    func onStatus(status: BridgeBootstrapStatus) {
        Task { @MainActor in apply(status) }
    }
}
