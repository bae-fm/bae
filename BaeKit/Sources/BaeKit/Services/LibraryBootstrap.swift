import Foundation
import Observation

/// One restore or invitation attempt, including the bridge's cancellation token.
@MainActor
@Observable
public final class LibraryBootstrap {
    public private(set) var status: BridgeBootstrapStatus?
    private let host: BridgeHost
    private let deviceName: String
    private var cancellation: BootstrapCancellation?
    private var attempt: UUID?

    public init(host: BridgeHost, deviceName: String) {
        self.host = host
        self.deviceName = deviceName
    }

    public func restore(code: String, credentials: BridgeS3Credentials?)
        async throws -> BridgeBootstrapResult
    {
        try await run { callback, cancellation in
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
        try await run { callback, cancellation in
            try await self.host.restoreFromKeychain(
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
        try await run { callback, cancellation in
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

    public func join(code: String, personName: String) async throws
        -> BridgeBootstrapResult
    {
        try await run { callback, cancellation in
            try await self.host.joinWithInvite(
                code: code,
                deviceName: self.deviceName,
                personName: personName,
                callback: callback,
                cancellation: cancellation
            )
        }
    }

    public func cancel() { cancellation?.cancel() }

    private func run(
        _ work: (BootstrapStatusSink, BootstrapCancellation) async throws ->
            BridgeBootstrapResult
    ) async throws -> BridgeBootstrapResult {
        precondition(cancellation == nil, "one bootstrap attempt at a time")
        let token = BootstrapCancellation()
        let identity = UUID()
        cancellation = token
        attempt = identity
        status = .connecting
        let sink = BootstrapStatusSink { [weak self] status in
            guard self?.attempt == identity else { return }
            self?.status = status
        }
        defer {
            attempt = nil
            cancellation = nil
            status = nil
        }
        // A published result owns a live store even if cancellation raced it.
        // Always deliver that result so its owner can finish opening or close it.
        return try await withTaskCancellationHandler {
            try await work(sink, token)
        } onCancel: {
            token.cancel()
        }
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
