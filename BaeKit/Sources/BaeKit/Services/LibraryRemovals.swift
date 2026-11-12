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
    /// The removal running from where libraries are listed, with its steps.
    public private(set) var removing: RunningRemoval?
    /// The library that isn't open which counting or removing it is opening
    /// now, with the open's steps.
    public private(set) var opening: ClosedLibraryOpening?
    /// A removal that left the store but not this device's files for it.
    /// A removal that stopped before the store was deleted shows only its
    /// `unfinishedError`: the library is still here, with nothing to finish.
    public private(set) var unfinished: BridgeUnfinishedRemoval?
    /// Why `unfinished` stopped, why reading it at launch failed, or why
    /// finishing it failed.
    public private(set) var unfinishedError: DisplayError?
    /// The last removal started from where libraries are listed that failed,
    /// for the person to retry or drop.
    public private(set) var failed: FailedRemoval?
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
        @Sendable (String, BridgeLibraryOpening) async throws -> Bool
    @ObservationIgnored
    private let removeClosed:
        @Sendable (String, BridgeLibraryOpening, BridgeLibraryClosing)
            async throws -> BridgeLibraryRemoval
    @ObservationIgnored
    private let removeUnopenable:
        @Sendable (String, BridgeLibraryClosing) async throws ->
            BridgeLibraryRemoval
    @ObservationIgnored
    private let finishDeletion:
        @Sendable (String, BridgeLibraryClosing) async throws ->
            BridgeLibraryRemoval
    @ObservationIgnored
    private let findUnfinished:
        @Sendable () async throws -> BridgeUnfinishedRemoval?
    @ObservationIgnored
    private let finishUnfinished:
        @Sendable (BridgeLibrarySetup) async throws -> Void
    @ObservationIgnored
    private var task: Task<Void, Never>?

    /// `unsentOfClosed` and `removeClosed` open the library, writing the
    /// open's steps into the opening's stream; `removeClosed`,
    /// `removeUnopenable` and `finishDeletion` write the removal's steps into
    /// the closing's, and `finishUnfinished` how many removals it finished
    /// into the setup's.
    public init(
        makeOpenProgress: @escaping @Sendable () -> LibraryOpenProgress,
        makeRemovalSteps: @escaping @Sendable () -> LibraryClosingSteps,
        makeFinishingSteps: @escaping @Sendable () -> LibrarySetupSteps,
        unsentOfClosed:
            @escaping @Sendable (String, BridgeLibraryOpening) async throws
            -> Bool,
        removeClosed:
            @escaping @Sendable (
                String, BridgeLibraryOpening, BridgeLibraryClosing
            ) async throws -> BridgeLibraryRemoval,
        removeUnopenable:
            @escaping @Sendable (String, BridgeLibraryClosing) async throws ->
            BridgeLibraryRemoval,
        finishDeletion:
            @escaping @Sendable (String, BridgeLibraryClosing) async throws ->
            BridgeLibraryRemoval,
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
        self.finishDeletion = finishDeletion
        self.findUnfinished = findUnfinished
        self.finishUnfinished = finishUnfinished
    }

    /// What removing `library`, which isn't open, would lose, for the
    /// person to confirm first; nil after a failure, kept as `failed`. A
    /// library that can't open isn't opened to count.
    public func loss(closedLibrary library: BridgeLibrary) async
        -> RemovalLoss?
    {
        failed = nil
        guard library.isOpenable else {
            logger.info(
                "\(library.id) can't open; removing it loses what it hasn't sent, uncounted"
            )
            return .uncounted
        }
        let progress = makeOpenProgress()
        opening = ClosedLibraryOpening(
            libraryId: library.id,
            progress: progress
        )
        defer { opening = nil }
        do {
            return .counted(
                holdsUnsent: try await unsentOfClosed(
                    library.id,
                    progress.opening
                )
            )
        }
        catch {
            logger.error(
                "Couldn't read what removing \(library.id) loses: \(String(reflecting: error))"
            )
            failed = DisplayError(error)
                .map {
                    FailedRemoval(library: library, error: $0, retry: .ask)
                }
            return nil
        }
    }

    /// Remove a library that isn't open, once the person confirmed `loss`,
    /// then report the list changed. A library that opens is opened to leave
    /// its store; one that can't is removed without opening.
    public func remove(
        closedLibrary library: BridgeLibrary,
        losing loss: RemovalLoss,
        onFinished: @escaping () -> Void
    ) {
        let libraryId = library.id
        run(library, retry: .remove(loss), onFinished: onFinished) { steps in
            switch loss {
            case .counted:
                let progress = self.makeOpenProgress()
                self.opening = ClosedLibraryOpening(
                    libraryId: libraryId,
                    progress: progress
                )
                defer { self.opening = nil }
                return try await self.removeClosed(
                    libraryId,
                    progress.opening,
                    steps.closing
                )
            case .uncounted:
                return try await self.removeUnopenable(libraryId, steps.closing)
            }
        }
    }

    /// Finish removing a library whose removal stopped partway, which
    /// never opens again, then report the list changed.
    public func finishRemoving(
        library: BridgeLibrary,
        onFinished: @escaping () -> Void
    ) {
        run(library, retry: .finish, onFinished: onFinished) { steps in
            try await self.finishDeletion(library.id, steps.closing)
        }
    }

    /// Run one removal of `library`, its steps on the library's row, and
    /// note how it ended; a failure is kept as `failed` with `retry`.
    private func run(
        _ library: BridgeLibrary,
        retry: FailedRemoval.Retry,
        onFinished: @escaping () -> Void,
        removal:
            @escaping (LibraryClosingSteps) async throws ->
            BridgeLibraryRemoval
    ) {
        precondition(task == nil, "one library removal at a time")
        let steps = makeRemovalSteps()
        removing = RunningRemoval(libraryId: library.id, steps: steps)
        failed = nil
        task = Task {
            defer {
                removing = nil
                task = nil
            }
            do {
                record(try await removal(steps))
            }
            catch {
                logger.error(
                    "Could not remove \(library.id): \(String(reflecting: error))"
                )
                failed = DisplayError(error)
                    .map {
                        FailedRemoval(library: library, error: $0, retry: retry)
                    }
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
            unfinishedError = nil
        case .unfinished(let leftover):
            show(leftover)
        }
    }

    private func show(_ removal: BridgeUnfinishedRemoval) {
        logger.error("A library removal didn't finish: \(removal.error)")
        unfinished = removal.filesRemain ? removal : nil
        unfinishedError = DisplayError(removal.error)
    }

    /// Drop the failure the person has seen.
    public func dismissFailure() {
        failed = nil
    }

    /// Delete the files unfinished removals left.
    public func retryUnfinished() {
        precondition(
            unfinished != nil,
            "Retry shows only for an unfinished removal"
        )
        precondition(task == nil, "one library removal at a time")
        unfinishedError = nil
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
                self.unfinishedError = DisplayError(error)
            }
        }
    }
}

extension LibraryRemovals {
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
            removeUnopenable: { libraryId, closing in
                try await host.removeUnopenableLibrary(
                    libraryId: libraryId,
                    closing: closing
                )
            },
            finishDeletion: { libraryId, closing in
                try await host.finishDeletion(
                    libraryId: libraryId,
                    closing: closing
                )
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
            self.unfinishedError = DisplayError(error)
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
}

/// A library that isn't open which counting or removing it is opening, with
/// the open's steps.
public struct ClosedLibraryOpening: Sendable {
    public let libraryId: String
    public let progress: LibraryOpenProgress
}

/// A removal of a library that isn't open, with the steps it runs once the
/// library is open, or at once for one that can't open.
public struct RunningRemoval: Sendable {
    public let libraryId: String
    public let steps: LibraryClosingSteps

    #if DEBUG
        /// A removal a shot shows running.
        public init(libraryId: String, steps: LibraryClosingSteps) {
            self.libraryId = libraryId
            self.steps = steps
        }
    #endif
}
