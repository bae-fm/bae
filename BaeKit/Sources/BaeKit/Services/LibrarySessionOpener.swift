import Foundation
import OSLog

private let logger = Logger.bae("LibrarySessionOpener")

/// The slice of an open `AppHandle` the session opener drives: read the config,
/// gate on the stored encryption key, seed the outbox, check sync readiness, and
/// tear the core down on a failed open. `AppHandle` satisfies it directly; the
/// narrow shape is the seam a unit test fakes, since the global `initApp` the
/// opener runs can't otherwise be stood up in a test.
public protocol LibrarySessionHandle: Sendable {
    func getConfig() -> BridgeConfig
    func hasEncryptionKey() -> Bool
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot
    func isSyncReady() -> Bool
    func shutdown() async throws
}

extension AppHandle: LibrarySessionHandle {}

/// Opens a local library into a wired `AppService`, superseding any open still
/// in flight. This is the platform-shared half of the launch / switch / unlock
/// flow that macOS's `AppDelegate` and iOS's `AppSessionHolder` used to keep in
/// lockstep by hand: run the off-main `initApp`, bail if a newer open superseded
/// this one, gate on the stored encryption key, seed the outbox mirror (tearing
/// the core down if that read fails), build and wire the service, and store the
/// restore code once sync is ready.
///
/// Each platform keeps its own shell (an `NSApplicationDelegate` vs an
/// `@Observable` holder) and maps the `Outcome` onto its own screen model. The
/// two seams are the `makeService` factory (which builds and wires the platform
/// `AppService` subclass) and the injected `makeHandle` (which wraps the global
/// `initApp`, so the opener never calls it directly and a test can stand in a
/// fake).
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
        slot.replace {
            let outcome = await self.run(libraryId: libraryId)
            onOutcome(outcome)
        }
    }

    /// Cancel any open still in flight without starting a new one — used when
    /// the caller tears its session down (close) so a parked `initApp` can't
    /// resume past its cancellation check and land a library after the close.
    public func cancel() {
        slot.cancel()
    }

    private func run(libraryId: String) async -> Outcome {
        let makeHandle = self.makeHandle
        do {
            let handle = try await DetachedWork.run {
                try makeHandle(libraryId)
            }
            // A newer open may have superseded this one while `initApp` ran
            // (cancelling this task). Bail before producing any outcome the
            // caller would act on; `handle` drops here, freeing the core.
            try Task.checkCancellation()
            let config = handle.getConfig()
            if config.encryptionKeyStored, !handle.hasEncryptionKey() {
                // Drop the handle before the caller shows the unlock gate:
                // releasing the only strong reference runs the core's teardown,
                // so an unlock retry spins a fresh `initApp` on the same library
                // rather than racing a still-live one.
                return .needsUnlock(config)
            }
            let initialOutbox: BridgeOutboxSnapshot
            do {
                initialOutbox = try await handle.getOutboxSnapshot()
            }
            catch {
                // Seeding the outbox mirror failed: tear the just-opened core
                // down before surfacing, rather than leaving it half-open.
                logger.error("Failed to seed outbox snapshot: \(error)")
                do {
                    try await handle.shutdown()
                }
                catch {
                    logger.error(
                        "Failed to shut down after outbox seeding failed: \(error)"
                    )
                    return .failed(error)
                }
                return .failed(error)
            }
            let service = makeService(handle, config, initialOutbox)
            if handle.isSyncReady() {
                service.storeRestoreCodeInKeychain(
                    libraryId: config.libraryId,
                    onError: { [weak service] message in
                        Task { @MainActor in
                            service?.showError(DisplayError(line: message))
                        }
                    }
                )
            }
            return .opened(service)
        }
        catch is CancellationError {
            return .superseded
        }
        catch {
            return .failed(error)
        }
    }
}
