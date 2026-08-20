import Foundation
import OSLog

private let logger = Logger.bae("LibrarySessionOpener")

/// The slice of an open `AppHandle` the session opener drives: read the config
/// and Coven-owned cloud-key state, unlock that same handle, seed the outbox,
/// check sync readiness, and tear the core down on a failed open. `AppHandle`
/// satisfies it directly; the narrow shape is the seam a unit test fakes.
public protocol LibrarySessionHandle: AnyObject, Sendable {
    func getConfig() -> BridgeConfig
    func cloudHomeKeyState() throws -> BridgeCloudHomeKeyState
    func unlockCloudHome(serializedCloudKey: String) async throws
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot
    func isSyncReady() -> Bool
    func shutdown() async throws
}

extension AppHandle: LibrarySessionHandle {
    public func unlockCloudHome(serializedCloudKey: String) async throws {
        try await unlockCloudHome(serializedMasterKey: serializedCloudKey)
    }
}

/// Opens a local library into a wired `AppService`, superseding any open still
/// in flight. This is the platform-shared half of the launch / switch / unlock
/// flow that macOS's `AppDelegate` and iOS's `AppSessionHolder` used to keep in
/// lockstep by hand: run the off-main `initApp`, bail if a newer open superseded
/// this one, gate on the stored encryption key, seed the outbox mirror (tearing
/// the core down if that read fails), build and wire the service, and store the
/// restore code once sync is ready.
///
/// Each platform keeps its own shell (an `NSApplicationDelegate` vs an
/// `@Observable` holder) and maps the `Outcome` onto its own screen model. A
/// locked handle remains owned here so entering a key completes the already
/// open Coven owner instead of constructing a second owner around the same
/// library.
@MainActor
public final class LibrarySessionOpener<
    Handle: LibrarySessionHandle,
    Service: AppService
> {
    /// The result of one open, mapped by the caller onto its screen model.
    /// `.superseded` means a newer open cancelled this one; the newer open owns
    /// the screen, so the caller does nothing — a locked target left the current
    /// session running, and cancelling returns to it.
    public enum Outcome {
        case opened(Service)
        case needsUnlock(BridgeConfig)
        case superseded
        case failed(Error)
    }

    /// Builds the core off the main actor. Injected so the opener doesn't call
    /// the global `initApp` directly and a test can substitute a fake handle.
    private let makeHandle: @Sendable (String) throws -> Handle

    /// Builds and wires the platform `AppService` around the opened handle,
    /// config, and seeded outbox snapshot.
    private let makeService:
        @MainActor (Handle, BridgeConfig, BridgeOutboxSnapshot) -> Service

    /// The open still in flight, cancelled by the next `open` (or an explicit
    /// `cancel`) so its now-stale result never lands on the caller's screen.
    private let slot = CancellableTaskSlot()

    private struct LockedSession {
        let id: UUID
        let handle: Handle
        let config: BridgeConfig
    }

    /// The one locked core awaiting its key. A switch or cancel releases it;
    /// an unlock failure retains it so the user can correct the key and retry.
    private var lockedSession: LockedSession?

    public init(
        makeHandle: @escaping @Sendable (String) throws -> Handle,
        makeService:
            @escaping @MainActor (
                Handle, BridgeConfig, BridgeOutboxSnapshot
            ) -> Service
    ) {
        self.makeHandle = makeHandle
        self.makeService = makeService
    }

    /// Open `libraryId`, superseding any open still in flight, and deliver the
    /// outcome on the main actor. The prior open is cancelled first, so a fast
    /// library switch can't land a stale library after the newer one.
    public func open(
        libraryId: String,
        onOutcome: @escaping @MainActor (Outcome) -> Void
    ) {
        lockedSession = nil
        slot.replace {
            let outcome = await self.run(libraryId: libraryId)
            onOutcome(outcome)
        }
    }

    /// Unlock the retained core and finish opening it. The handle that reported
    /// `.locked` performs the operation; no global key write or second
    /// `initApp` participates.
    public func unlock(serializedCloudKey: String) async throws -> Service {
        guard let locked = lockedSession else {
            throw CancellationError()
        }
        try await locked.handle.unlockCloudHome(
            serializedCloudKey: serializedCloudKey
        )
        try Task.checkCancellation()
        guard lockedSession?.id == locked.id else {
            throw CancellationError()
        }
        lockedSession = nil
        return try await finish(
            handle: locked.handle,
            config: locked.config
        )
    }

    /// Cancel any open still in flight without starting a new one — used when
    /// the caller tears its session down (close) so a parked `initApp` can't
    /// resume past its cancellation check and land a library after the close.
    public func cancel() {
        slot.cancel()
        lockedSession = nil
    }

    private func run(libraryId: String) async -> Outcome {
        let makeHandle = self.makeHandle
        logger.info("open: building core for library \(libraryId)")
        do {
            let handle = try await DetachedWork.run {
                try makeHandle(libraryId)
            }
            logger.info("open: core built")
            // A newer open may have superseded this one while `initApp` ran
            // (cancelling this task). Bail before producing any outcome the
            // caller would act on; `handle` drops here, freeing the core.
            try Task.checkCancellation()
            let config = handle.getConfig()
            if try handle.cloudHomeKeyState() == .locked {
                lockedSession = LockedSession(
                    id: UUID(),
                    handle: handle,
                    config: config
                )
                return .needsUnlock(config)
            }
            return .opened(try await finish(handle: handle, config: config))
        }
        catch is CancellationError {
            return .superseded
        }
        catch {
            return .failed(error)
        }
    }

    private func finish(
        handle: Handle,
        config: BridgeConfig
    ) async throws -> Service {
        let initialOutbox: BridgeOutboxSnapshot
        logger.info("open: seeding outbox snapshot")
        do {
            initialOutbox = try await handle.getOutboxSnapshot()
            logger.info("open: outbox snapshot seeded")
        }
        catch {
            logger.error("Failed to seed outbox snapshot: \(error)")
            do {
                try await handle.shutdown()
            }
            catch {
                logger.error(
                    "Failed to shut down after outbox seeding failed: \(error)"
                )
                throw error
            }
            throw error
        }
        let service = makeService(handle, config, initialOutbox)
        logger.info("open: service built and wired")
        if handle.isSyncReady() {
            service.storeRestoreCodeInKeychain(
                libraryId: config.libraryId,
                onError: { [weak service] failure in
                    Task { @MainActor in
                        service?.showError(failure)
                    }
                }
            )
        }
        return service
    }
}
