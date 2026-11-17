import Foundation
import OSLog

private let logger = Logger.bae("LibrarySessionOpener")

public protocol LibrarySessionHandle: AnyObject, Sendable {
    func getConfig() async throws -> BridgeConfig
    func storageLocation() async throws -> BridgeStorageLocation?
    func getUploadsPaused() async throws -> Bool
    func closeLibrary(closing: BridgeLibraryClosing?) throws
}

extension AppHandle: LibrarySessionHandle {}

public struct LibrarySessionSeed: Sendable {
    public let storage: BridgeStorageLocation?
    public let uploadsPaused: Bool

    public init(
        storage: BridgeStorageLocation?,
        uploadsPaused: Bool
    ) {
        self.storage = storage
        self.uploadsPaused = uploadsPaused
    }

    static func read(from handle: some LibrarySessionHandle) async throws
        -> Self
    {
        let storage = try await handle.storageLocation()
        let uploadsPaused = try await handle.getUploadsPaused()
        return Self(storage: storage, uploadsPaused: uploadsPaused)
    }
}

/// Owns opening and seeding a session. Bootstrap callers supply their retained
/// handle; existing local libraries use the injected factory.
@MainActor
public final class LibrarySessionOpener<
    Handle: LibrarySessionHandle,
    Service: AppService
> {
    public enum Outcome {
        case opened(Service)
        case keychainLocked
        case superseded
        case failed(Error)
    }

    private let makeHandle:
        @Sendable (String, BridgeLibraryOpening) throws -> Handle
    private let makeService:
        @MainActor (Handle, BridgeConfig, LibrarySessionSeed) -> Service
    private let slot = CancellableTaskSlot()

    /// `makeHandle` opens a library by id, writing the open's steps into the
    /// opening's stream.
    public init(
        makeHandle:
            @escaping @Sendable (String, BridgeLibraryOpening) throws -> Handle,
        makeService:
            @escaping @MainActor (Handle, BridgeConfig, LibrarySessionSeed) ->
            Service
    ) {
        self.makeHandle = makeHandle
        self.makeService = makeService
    }

    /// Open the library `libraryId`, its steps followed through `progress`.
    public func open(
        libraryId: String,
        progress: LibraryOpenProgress,
        onOutcome: @escaping @MainActor (Outcome) -> Void
    ) {
        let factory = makeHandle
        open(
            makeHandle: { try factory(libraryId, progress.opening) },
            onOutcome: onOutcome
        )
    }

    public func open(
        makeHandle: @escaping @Sendable () throws -> Handle,
        onOutcome: @escaping @MainActor (Outcome) -> Void
    ) {
        slot.replace {
            do {
                let handle = try await DetachedWork.run(makeHandle)
                do {
                    try Task.checkCancellation()
                    let seed = try await LibrarySessionSeed.read(from: handle)
                    let config = try await handle.getConfig()
                    try Task.checkCancellation()
                    onOutcome(
                        .opened(self.makeService(handle, config, seed))
                    )
                }
                catch {
                    logger.error("Failed to seed library: \(error)")
                    try await DetachedWork.run {
                        try handle.closeLibrary(closing: nil)
                    }
                    throw error
                }
            }
            catch is CancellationError {
                onOutcome(.superseded)
            }
            catch {
                if Self.keychainLocked(error) {
                    onOutcome(.keychainLocked)
                }
                else {
                    onOutcome(.failed(error))
                }
            }
        }
    }

    public func cancel() { slot.cancel() }

    /// Whether `error` says the keychain is locked now, so the opener tries
    /// again after the person unlocks the keychain. Core fails an open by id
    /// as `BridgeOpenError.KeychainLocked`. Opening what a create, restore or
    /// reset returned fails with a `BridgeError` whose category says so.
    private static func keychainLocked(_ error: Error) -> Bool {
        switch error {
        case BridgeOpenError.KeychainLocked: true
        case BridgeError.Diagnostic(.keyringLocked, _): true
        default: false
        }
    }
}
