import Observation
import os.log

private let logger = Logger.bae("LibraryRemovals")

/// Removing libraries from this device, what a removal would lose for the
/// person to confirm first, and a removal that left the store but not this
/// device's files for it. Core reads that last one from disk, so it
/// shows again after the app restarts. Process-wide, so a removal started in
/// Settings and one started where libraries are listed finish in the same
/// place.
@MainActor
@Observable
public final class LibraryRemovals {
    /// The library whose removal is running from where libraries are listed.
    public private(set) var removingLibraryId: String?
    /// A removal that left the store but not this device's files for it.
    /// A removal that stopped before the store was deleted shows only its
    /// `error`: the library is still here, with nothing to finish.
    public private(set) var unfinished: BridgeUnfinishedRemoval?
    /// The last removal failure, or why `unfinished` stopped.
    public private(set) var error: DisplayError?
    public private(set) var isRetrying = false

    @ObservationIgnored
    private let unsentOfClosed:
        @Sendable (String) async throws -> BridgeUnsentWork
    @ObservationIgnored
    private let removeClosed:
        @Sendable (String) async throws -> BridgeLibraryRemoval
    @ObservationIgnored
    private let removeUnopenable:
        @Sendable (String) async throws -> BridgeLibraryRemoval
    @ObservationIgnored
    private let findUnfinished:
        @Sendable () async throws -> BridgeUnfinishedRemoval?
    @ObservationIgnored
    private let finishUnfinished: @Sendable () async throws -> Void
    @ObservationIgnored
    private var task: Task<Void, Never>?

    public init(
        unsentOfClosed:
            @escaping @Sendable (String) async throws -> BridgeUnsentWork,
        removeClosed:
            @escaping @Sendable (String) async throws -> BridgeLibraryRemoval,
        removeUnopenable:
            @escaping @Sendable (String) async throws -> BridgeLibraryRemoval,
        findUnfinished:
            @escaping @Sendable () async throws -> BridgeUnfinishedRemoval?,
        finishUnfinished: @escaping @Sendable () async throws -> Void
    ) {
        self.unsentOfClosed = unsentOfClosed
        self.removeClosed = removeClosed
        self.removeUnopenable = removeUnopenable
        self.findUnfinished = findUnfinished
        self.finishUnfinished = finishUnfinished
    }

    /// The production wiring over the process's host.
    public static func live(host: BridgeHost) -> LibraryRemovals {
        LibraryRemovals(
            unsentOfClosed: { libraryId in
                try await host.unsentBeforeRemoval(libraryId: libraryId)
            },
            removeClosed: { libraryId in
                try await removeClosedLibrary(libraryId, host: host)
            },
            removeUnopenable: { libraryId in
                try await host.removeUnopenableLibrary(libraryId: libraryId)
            },
            findUnfinished: { try await host.unfinishedRemoval() },
            finishUnfinished: { try await host.finishRemovals() }
        )
    }

    /// Show a removal an earlier run of the app left unfinished, if any.
    public func loadUnfinished() async {
        do {
            if let removal = try await findUnfinished() {
                show(removal)
            }
        }
        catch {
            logger.error(
                "Couldn't read unfinished library removals: \(error.localizedDescription)"
            )
            self.error = DisplayError(error)
        }
    }

    /// Leaving the store runs on an open handle, so the library opens first.
    /// A removal that failed closes the handle it opened.
    nonisolated public static func removeClosedLibrary(
        _ libraryId: String,
        host: BridgeHost
    ) async throws -> BridgeLibraryRemoval {
        let app = try await DetachedWork.run {
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
                restorePlayback: false,
                host: host
            )
        }
        do {
            return try await app.removeLibrary()
        }
        catch {
            await close(app, libraryId: libraryId)
            throw error
        }
    }

    nonisolated private static func close(
        _ app: AppHandle,
        libraryId: String
    ) async {
        do {
            try await DetachedWork.run { try app.closeLibrary() }
        }
        catch {
            logger.error(
                "Couldn't close \(libraryId) after its removal stopped: \(error.localizedDescription)"
            )
        }
    }

    /// What removing `library`, which isn't open, would lose, for the
    /// person to confirm first; nil after a failure, shown as `error`. A
    /// library that can't open isn't opened to count.
    public func loss(closedLibrary library: BridgeLibrary) async
        -> RemovalLoss?
    {
        error = nil
        guard library.isOpenable else {
            logger.info(
                "\(library.id) can't open; removing it loses what it hasn't sent, uncounted"
            )
            return .uncounted
        }
        do {
            return .unsent(try await unsentOfClosed(library.id))
        }
        catch {
            logger.error(
                "Couldn't read what removing \(library.id) loses: \(error.localizedDescription)"
            )
            self.error = DisplayError(error)
            return nil
        }
    }

    /// Remove a library that isn't open, once the person confirmed `loss`,
    /// then report the list changed. A library that opens is opened to leave
    /// its store; one that can't is removed without opening.
    public func remove(
        closedLibrary libraryId: String,
        losing loss: RemovalLoss,
        onFinished: @escaping () -> Void
    ) {
        precondition(task == nil, "one library removal at a time")
        removingLibraryId = libraryId
        error = nil
        task = Task {
            defer {
                removingLibraryId = nil
                task = nil
            }
            do {
                switch loss {
                case .unsent:
                    record(try await removeClosed(libraryId))
                case .uncounted:
                    record(try await removeUnopenable(libraryId))
                }
            }
            catch {
                logger.error(
                    "Couldn't remove \(libraryId): \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
            onFinished()
        }
    }

    /// Note how a removal ended: this device's files for it may remain to be
    /// deleted.
    public func record(_ removal: BridgeLibraryRemoval) {
        switch removal {
        case .removed:
            unfinished = nil
            error = nil
        case .unfinished(let leftover):
            show(leftover)
        }
    }

    private func show(_ removal: BridgeUnfinishedRemoval) {
        logger.error("A library removal didn't finish: \(removal.error)")
        unfinished = removal.filesRemain ? removal : nil
        error = DisplayError(removal.error)
    }

    /// Delete the files unfinished removals left.
    public func retryUnfinished() {
        precondition(
            unfinished != nil,
            "Retry shows only for an unfinished removal"
        )
        precondition(task == nil, "one library removal at a time")
        error = nil
        isRetrying = true
        task = Task {
            defer {
                isRetrying = false
                task = nil
            }
            do {
                try await finishUnfinished()
                unfinished = nil
            }
            catch {
                logger.error(
                    "Couldn't finish removing a library: \(error.localizedDescription)"
                )
                self.error = DisplayError(error)
            }
        }
    }
}
