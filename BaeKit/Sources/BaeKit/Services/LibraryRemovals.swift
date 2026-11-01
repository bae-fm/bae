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
    /// The library that isn't open which counting or removing it is opening
    /// now, with the open's steps.
    public private(set) var opening: ClosedLibraryOpening?
    /// A removal that left the store but not this device's files for it.
    /// A removal that stopped before the store was deleted shows only its
    /// `error`: the library is still here, with nothing to finish.
    public private(set) var unfinished: BridgeUnfinishedRemoval?
    /// The last removal failure, or why `unfinished` stopped.
    public private(set) var error: DisplayError?
    /// Finishing unfinished removals, with its steps; nil while it doesn't
    /// run.
    public private(set) var finishing: LibrarySetupSteps?
    public var isRetrying: Bool { finishing != nil }

    /// A new open of a closed library, whose steps the open's view draws.
    @ObservationIgnored
    private let makeOpenProgress: @Sendable () -> LibraryOpenProgress
    /// A new removal's steps, which the removed library's row draws.
    @ObservationIgnored
    private let makeRemovalSteps: @Sendable () -> LibraryClosingSteps
    /// A new finishing of unfinished removals, whose steps the failure's
    /// notice draws.
    @ObservationIgnored
    private let makeFinishingSteps: @Sendable () -> LibrarySetupSteps
    @ObservationIgnored
    private let unsentOfClosed:
        @Sendable (String, BridgeLibraryOpening) async throws
            -> BridgeUnsentWork
    @ObservationIgnored
    private let removeClosed:
        @Sendable (String, BridgeLibraryOpening, BridgeLibraryClosing)
            async throws -> BridgeLibraryRemoval
    @ObservationIgnored
    private let removeUnopenable:
        @Sendable (String) async throws -> BridgeLibraryRemoval
    @ObservationIgnored
    private let findUnfinished:
        @Sendable () async throws -> BridgeUnfinishedRemoval?
    @ObservationIgnored
    private let finishUnfinished:
        @Sendable (BridgeLibrarySetup) async throws -> Void
    @ObservationIgnored
    private var task: Task<Void, Never>?

    /// `unsentOfClosed` and `removeClosed` open the library, writing the
    /// open's steps into the opening's stream; `removeClosed` writes the
    /// removal's steps into the closing's, and `finishUnfinished` how many
    /// removals it finished into the setup's.
    public init(
        makeOpenProgress: @escaping @Sendable () -> LibraryOpenProgress,
        makeRemovalSteps: @escaping @Sendable () -> LibraryClosingSteps,
        makeFinishingSteps: @escaping @Sendable () -> LibrarySetupSteps,
        unsentOfClosed:
            @escaping @Sendable (String, BridgeLibraryOpening) async throws
            -> BridgeUnsentWork,
        removeClosed:
            @escaping @Sendable (
                String, BridgeLibraryOpening, BridgeLibraryClosing
            ) async throws -> BridgeLibraryRemoval,
        removeUnopenable:
            @escaping @Sendable (String) async throws -> BridgeLibraryRemoval,
        findUnfinished:
            @escaping @Sendable () async throws -> BridgeUnfinishedRemoval?,
        finishUnfinished:
            @escaping @Sendable (BridgeLibrarySetup) async throws -> Void
    ) {
        self.makeOpenProgress = makeOpenProgress
        self.makeRemovalSteps = makeRemovalSteps
        self.makeFinishingSteps = makeFinishingSteps
        self.unsentOfClosed = unsentOfClosed
        self.removeClosed = removeClosed
        self.removeUnopenable = removeUnopenable
        self.findUnfinished = findUnfinished
        self.finishUnfinished = finishUnfinished
    }

    /// The production wiring over the process's host.
    public static func live(host: BridgeHost) -> LibraryRemovals {
        LibraryRemovals(
            makeOpenProgress: { LibraryOpenProgress(host: host) },
            makeRemovalSteps: { LibraryClosingSteps(host: host) },
            makeFinishingSteps: { LibrarySetupSteps(host: host) },
            unsentOfClosed: { libraryId, opening in
                try await host.unsentBeforeRemoval(
                    libraryId: libraryId,
                    opening: opening
                )
            },
            removeClosed: { libraryId, opening, closing in
                try await removeClosedLibrary(
                    libraryId,
                    opening: opening,
                    closing: closing,
                    host: host
                )
            },
            removeUnopenable: { libraryId in
                try await host.removeUnopenableLibrary(libraryId: libraryId)
            },
            findUnfinished: { try await host.unfinishedRemoval() },
            finishUnfinished: { setup in
                try await host.finishRemovals(setup: setup)
            }
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
                "Couldn't read unfinished library removals: \(String(reflecting: error))"
            )
            self.error = DisplayError(error)
        }
    }

    /// Leaving the store runs on an open handle, so the library opens first,
    /// writing its steps to `opening`, then the removal writes its own to
    /// `closing`. A removal that failed closes the handle it opened.
    nonisolated private static func removeClosedLibrary(
        _ libraryId: String,
        opening: BridgeLibraryOpening,
        closing: BridgeLibraryClosing,
        host: BridgeHost
    ) async throws -> BridgeLibraryRemoval {
        let app = try await DetachedWork.run {
            try initApp(
                libraryId: libraryId,
                positionUpdateIntervalMs: 200,
                restorePlayback: false,
                host: host,
                opening: opening
            )
        }
        do {
            return try await app.removeLibrary(closing: closing)
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
            try await DetachedWork.run { try app.closeLibrary(closing: nil) }
        }
        catch {
            logger.error(
                "Couldn't close \(libraryId) after its removal stopped: \(String(reflecting: error))"
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
        let progress = makeOpenProgress()
        opening = ClosedLibraryOpening(
            libraryId: library.id,
            progress: progress,
            removal: nil
        )
        defer { opening = nil }
        do {
            return .unsent(
                try await unsentOfClosed(library.id, progress.opening)
            )
        }
        catch {
            logger.error(
                "Couldn't read what removing \(library.id) loses: \(String(reflecting: error))"
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
                    let progress = makeOpenProgress()
                    let removal = makeRemovalSteps()
                    opening = ClosedLibraryOpening(
                        libraryId: libraryId,
                        progress: progress,
                        removal: removal
                    )
                    defer { opening = nil }
                    record(
                        try await removeClosed(
                            libraryId,
                            progress.opening,
                            removal.closing
                        )
                    )
                case .uncounted:
                    record(try await removeUnopenable(libraryId))
                }
            }
            catch {
                logger.error(
                    "Couldn't remove \(libraryId): \(String(reflecting: error))"
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
        let steps = makeFinishingSteps()
        finishing = steps
        task = Task {
            defer {
                finishing = nil
                task = nil
            }
            do {
                try await finishUnfinished(steps.setup)
                unfinished = nil
            }
            catch {
                logger.error(
                    "Couldn't finish removing a library: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// A library that isn't open which counting or removing it is opening, with
/// the open's steps, and when removing it, the removal's.
public struct ClosedLibraryOpening: Sendable {
    public let libraryId: String
    public let progress: LibraryOpenProgress
    public let removal: LibraryClosingSteps?
}
