import Foundation
import OSLog

private let logger = Logger.bae("LibrarySessionOpener")

public protocol LibrarySessionHandle: AnyObject, Sendable {
    func getConfig() -> BridgeConfig
    func libraryName() async throws -> String
    func storageLocation() async throws -> BridgeStorageLocation?
    func getOutboxSnapshot() async throws -> BridgeOutboxSnapshot
    func getDownloadSnapshot() async throws -> BridgeDownloadSnapshot
    #if os(macOS)
        func getOutputSnapshot() async throws -> BridgeOutputSnapshot
    #endif
    func shutdown() async throws
}

extension AppHandle: LibrarySessionHandle {}

public struct LibrarySessionSeed: Sendable {
    public let name: String
    public let storage: BridgeStorageLocation?
    public let outbox: BridgeOutboxSnapshot
    public let downloads: BridgeDownloadSnapshot
    #if os(macOS)
        public let outputs: BridgeOutputSnapshot
        public init(
            name: String,
            storage: BridgeStorageLocation?,
            outbox: BridgeOutboxSnapshot,
            downloads: BridgeDownloadSnapshot,
            outputs: BridgeOutputSnapshot
        ) {
            self.name = name
            self.storage = storage
            self.outbox = outbox
            self.downloads = downloads
            self.outputs = outputs
        }
    #else
        public init(
            name: String,
            storage: BridgeStorageLocation?,
            outbox: BridgeOutboxSnapshot,
            downloads: BridgeDownloadSnapshot
        ) {
            self.name = name
            self.storage = storage
            self.outbox = outbox
            self.downloads = downloads
        }
    #endif

    static func read(from handle: some LibrarySessionHandle) async throws
        -> Self
    {
        let name = try await handle.libraryName()
        let storage = try await handle.storageLocation()
        let outbox = try await handle.getOutboxSnapshot()
        let downloads = try await handle.getDownloadSnapshot()
        #if os(macOS)
            let outputs = try await handle.getOutputSnapshot()
            return Self(
                name: name,
                storage: storage,
                outbox: outbox,
                downloads: downloads,
                outputs: outputs
            )
        #else
            return Self(
                name: name,
                storage: storage,
                outbox: outbox,
                downloads: downloads
            )
        #endif
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

    private let makeHandle: @Sendable (String) throws -> Handle
    private let makeService:
        @MainActor (Handle, BridgeConfig, LibrarySessionSeed) -> Service
    private let slot = CancellableTaskSlot()

    public init(
        makeHandle: @escaping @Sendable (String) throws -> Handle,
        makeService:
            @escaping @MainActor (Handle, BridgeConfig, LibrarySessionSeed) ->
            Service
    ) {
        self.makeHandle = makeHandle
        self.makeService = makeService
    }

    public func open(
        libraryId: String,
        onOutcome: @escaping @MainActor (Outcome) -> Void
    ) {
        let factory = makeHandle
        open(makeHandle: { try factory(libraryId) }, onOutcome: onOutcome)
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
                    try Task.checkCancellation()
                    onOutcome(
                        .opened(
                            self.makeService(handle, handle.getConfig(), seed)
                        )
                    )
                }
                catch {
                    logger.error("Failed to seed library: \(error)")
                    try await handle.shutdown()
                    throw error
                }
            }
            catch is CancellationError {
                onOutcome(.superseded)
            }
            catch {
                if case BridgeError.Diagnostic(let category, _) = error,
                    category == .keyringLocked
                {
                    onOutcome(.keychainLocked)
                }
                else {
                    onOutcome(.failed(error))
                }
            }
        }
    }

    public func cancel() { slot.cancel() }
}
